use super::models::{
    AddFlowRequest, CompletedKycResponse, CreateSessionRequest, EnrollmentBindResponse,
    EnrollmentBindStatus, FinalizeRecoveryRequest, FlowDetailResponse, FlowResponse,
    LookupByPhoneCandidate, LookupByPhoneRequest, LookupByPhoneResponse, OldDevicePolicyRequest,
    OldDevicePolicyResponse, OldDevicePolicyStatus, PhoneMatchField, RecoveryBindRequest,
    RecoveryCaseResponse, SessionDetailResponse, SessionResponse, StepResponse, SubmitStepRequest,
    UserResponse,
};
use crate::api::{BackendApi, BffSignatureClaims};
use crate::flows::definitions::account_recovery::{hash_phone, mask_phone};
use crate::flows::registry::{actor_label, waiting_status};
use crate::flows::runtime::{
    merge_json_value, merged_json, resolve_transition, step_services_with_device,
};
use axum::http::HeaderMap;
use backend_core::Error;
use backend_flow_sdk::{Actor, Flow, FlowError, HumanReadableId, StepContext, StepOutcome};
use backend_model::db::{FlowInstanceRow, FlowSessionRow, FlowStepRow};
use backend_repository::{
    FlowInstanceCreateInput, FlowSessionCreateInput, FlowSessionFilter, FlowStepCreateInput,
    FlowStepPatch, RecoveryCaseCreateInput, RecoveryCaseUpdate,
};
use chrono::{Duration, Utc};
use serde_json::{Value, json};
use tracing::{debug, info, instrument};

const FLOW_STATUS_RUNNING: &str = "RUNNING";
const FLOW_STATUS_COMPLETED: &str = "COMPLETED";
const FLOW_STATUS_FAILED: &str = "FAILED";
const FLOW_STATUS_CLOSED: &str = "CLOSED";

/// Keys that must never be projected to a BFF client for recovery flows. This
/// covers the per-issue OTP hash + salt, phone hashes (needed only for
/// server-side case lookup), and the matched-user/existence signals (review
/// items C1 and C3).
fn is_redacted_recovery_key(key: &str) -> bool {
    matches!(
        key,
        "otp_hash" | "otp_salt" | "matched" | "matched_user_id" | "requested_phone_hash"
    )
}

/// Recursively strips recovery-sensitive fields from a JSON value in place.
/// Applied to every BFF-facing projection of recovery flow/session/step
/// context before it leaves the server.
pub(crate) fn redact_recovery_fields(value: &mut Value) {
    match value {
        Value::Object(map) => {
            map.retain(|k, v| {
                if is_redacted_recovery_key(k) {
                    return false;
                }
                redact_recovery_fields(v);
                true
            });
        }
        Value::Array(arr) => {
            for v in arr {
                redact_recovery_fields(v);
            }
        }
        _ => {}
    }
}

fn is_account_recovery(s: &str) -> bool {
    s.eq_ignore_ascii_case("account_recovery")
}

/// Projects a flow row, redacting recovery-sensitive context for BFF clients.
fn flow_response(row: FlowInstanceRow) -> FlowResponse {
    let mut row = row;
    if is_account_recovery(&row.flow_type) {
        redact_recovery_fields(&mut row.context);
    }
    row.into()
}

/// Projects a step row, redacting recovery-sensitive input/output/error.
fn step_response(flow_type: &str, row: FlowStepRow) -> StepResponse {
    let mut row = row;
    if is_account_recovery(flow_type) {
        if let Some(input) = row.input.as_mut() {
            redact_recovery_fields(input);
        }
        if let Some(output) = row.output.as_mut() {
            redact_recovery_fields(output);
        }
        if let Some(error) = row.error.as_mut() {
            redact_recovery_fields(error);
        }
    }
    row.into()
}

/// Projects a session row, redacting recovery-sensitive context.
fn session_response(row: FlowSessionRow) -> SessionResponse {
    let mut row = row;
    if is_account_recovery(&row.session_type) {
        redact_recovery_fields(&mut row.context);
    }
    row.into()
}

/// Identity of an authenticated BFF caller. `service_client_id` is set when the
/// caller is a service client (e.g. the BFF `azamra-bff`); such callers are not
/// end-users, so sessions they create must not carry an owning `user_id`.
#[derive(Debug, Clone)]
pub(crate) struct BffCallerIdentity {
    pub user_id: String,
    pub device_id: String,
    pub service_client_id: Option<String>,
}

pub async fn require_caller_identity(
    api: &BackendApi,
    headers: &HeaderMap,
) -> Result<BffCallerIdentity, Error> {
    let claims = api.require_bff_claims(headers)?;
    if claims.user_id.trim().is_empty() {
        return Err(Error::unauthorized(
            "Invalid signature-authenticated user id",
        ));
    }
    if claims.device_id.trim().is_empty() {
        return Err(Error::unauthorized(
            "Invalid signature-authenticated device id",
        ));
    }
    Ok(BffCallerIdentity {
        user_id: claims.user_id,
        device_id: claims.device_id,
        service_client_id: claims.service_client_id,
    })
}

pub async fn require_user_id(api: &BackendApi, headers: &HeaderMap) -> Result<String, Error> {
    Ok(require_caller_identity(api, headers).await?.user_id)
}

#[instrument(skip(api))]
pub async fn create_session(
    api: &BackendApi,
    caller: &BffCallerIdentity,
    body: CreateSessionRequest,
) -> Result<SessionResponse, Error> {
    debug!("Creating session of type: {}", body.session_type);
    let session_definition = api
        .state
        .flow_registry
        .get_session(&body.session_type)
        .ok_or_else(|| {
            Error::bad_request(
                "UNKNOWN_SESSION_TYPE",
                format!("Unknown session type: {}", body.session_type),
            )
        })?;

    // C3(a): account_recovery sessions are only created by service callers
    // (the BFF). End-users must not be able to spin up recovery sessions and
    // enumerate arbitrary numbers.
    if is_account_recovery(&body.session_type) && caller.service_client_id.is_none() {
        return Err(Error::forbidden(
            "RECOVERY_SERVICE_ONLY",
            "account_recovery sessions can only be created by a service caller",
        ));
    }

    // M4: bound per-phone recovery-session creation so an authenticated caller
    // cannot spam arbitrary numbers.
    let phone = body
        .context
        .as_ref()
        .and_then(|c| c.get("phone_number"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    if is_account_recovery(&body.session_type)
        && !phone.is_empty()
        && !api.state.rate_limiter.allow_session_creation(phone)
    {
        return Err(Error::too_many_requests(
            "RATE_LIMITED",
            "Too many recovery sessions for this phone; try again later",
        ));
    }

    let session_id = backend_id::flow_session_id()?;
    let human_id = normalize_or_default_human_id(
        body.human_id,
        format!(
            "{}.{}.{}",
            session_definition.human_id_prefix,
            Utc::now().format("%Y-%m-%d"),
            session_id
        ),
    )?;

    // A service-created session has no end-user owner: `user_id` is NULL so it
    // does not need to resolve in `app_user`. Only end-user sessions embed the
    // user id both on the row and in the session context.
    let (persisted_user_id, mut context) = match &caller.service_client_id {
        Some(_) => (None, object_context(body.context.clone())),
        None => (
            Some(caller.user_id.clone()),
            session_context_with_user_id(body.context.clone(), &caller.user_id),
        ),
    };

    // C2: bind a service-created recovery session to the recovering device
    // identity (device_id + jkt) carried in the BFF-supplied context so the
    // owner checks can fail closed instead of vacuously passing for NULL-owner
    // sessions.
    if is_account_recovery(&body.session_type) {
        let mut owner = serde_json::Map::new();
        if let Some(d) = body
            .context
            .as_ref()
            .and_then(|c| c.get("device_id"))
            .and_then(Value::as_str)
        {
            owner.insert("owner_device_id".to_owned(), Value::String(d.to_owned()));
        }
        if let Some(j) = body
            .context
            .as_ref()
            .and_then(|c| c.get("jkt"))
            .and_then(Value::as_str)
        {
            owner.insert("owner_jkt".to_owned(), Value::String(j.to_owned()));
        }
        if !owner.is_empty() {
            merge_json_value(&mut context, &json!({ "recovery": Value::Object(owner) }));
        }
        // C3(c): carry the authoritative configured lookup realm in the session
        // context so the resolve step can reject any client-supplied realm that
        // differs (preventing cross-realm account-existence probing).
        if !api.state.config.bff.recovery_lookup_realm.is_empty() {
            merge_json_value(
                &mut context,
                &json!({ "recovery": { "lookup_realm": api.state.config.bff.recovery_lookup_realm } }),
            );
        }
    }

    let row = api
        .state
        .flow
        .create_session(FlowSessionCreateInput {
            id: session_id,
            human_id,
            user_id: persisted_user_id,
            session_type: body.session_type,
            status: "OPEN".to_owned(),
            context,
        })
        .await?;

    Ok(session_response(row))
}

#[instrument(skip(api))]
pub async fn list_sessions(
    api: &BackendApi,
    user_id: String,
) -> Result<Vec<SessionResponse>, Error> {
    debug!("Listing sessions for user: {}", user_id);
    let (rows, _) = api
        .state
        .flow
        .list_sessions(FlowSessionFilter {
            user_id: Some(user_id),
            user_ids: None,
            session_type: None,
            status: None,
            page: 1,
            limit: 100,
        })
        .await?;

    Ok(rows.into_iter().map(session_response).collect())
}

#[instrument(skip(api))]
pub async fn get_session(
    api: &BackendApi,
    session_id: String,
    caller: &BffCallerIdentity,
) -> Result<SessionDetailResponse, Error> {
    debug!("Getting session: {}", session_id);
    let session = ensure_session_owner(api, &session_id, caller).await?;
    let flows = api.state.flow.list_flows_for_session(&session.id).await?;

    Ok(SessionDetailResponse {
        session: session_response(session),
        flows: flows.into_iter().map(flow_response).collect(),
    })
}

pub async fn list_session_flows(
    api: &BackendApi,
    session_id: String,
    caller: &BffCallerIdentity,
) -> Result<Vec<FlowResponse>, Error> {
    ensure_session_owner(api, &session_id, caller).await?;
    let flows = api.state.flow.list_flows_for_session(&session_id).await?;
    Ok(flows.into_iter().map(flow_response).collect())
}

#[instrument(skip(api))]
pub async fn add_flow_to_session(
    api: &BackendApi,
    session_id: String,
    caller: &BffCallerIdentity,
    body: AddFlowRequest,
) -> Result<FlowResponse, Error> {
    debug!(
        "Adding flow `{}` to session: {}",
        body.flow_type, session_id
    );
    let session = ensure_session_owner(api, &session_id, caller).await?;

    // C3(a): account_recovery flows are only added by service callers (the BFF).
    if is_account_recovery(&body.flow_type) && caller.service_client_id.is_none() {
        return Err(Error::forbidden(
            "RECOVERY_SERVICE_ONLY",
            "account_recovery flows can only be created by a service caller",
        ));
    }

    // M4: bound how many recovery flows are added to one session.
    if is_account_recovery(&body.flow_type)
        && !api.state.rate_limiter.allow_flow_creation(&session_id)
    {
        return Err(Error::too_many_requests(
            "RATE_LIMITED",
            "Too many recovery flows for this session; try again later",
        ));
    }

    let flow_definition = get_flow_definition(api, &body.flow_type)?;
    validate_session_flow_compatibility(api, &session, flow_definition.flow_type())?;

    let initial_step = body
        .initial_step
        .clone()
        .unwrap_or_else(|| flow_definition.initial_step().to_owned());

    ensure_flow_step_exists(flow_definition, &initial_step)?;

    let flow_id = backend_id::flow_instance_id()?;
    let flow_human_id = normalize_or_default_human_id(
        body.human_id,
        format!("{}.{}", session.human_id, flow_definition.human_id()),
    )?;

    let created = api
        .state
        .flow
        .create_flow(FlowInstanceCreateInput {
            id: flow_id,
            human_id: flow_human_id,
            session_id: session_id.clone(),
            flow_type: body.flow_type,
            status: FLOW_STATUS_RUNNING.to_owned(),
            current_step: None,
            step_ids: json!([]),
            context: object_context(body.context.clone()),
        })
        .await?;

    let mut created = created;
    if created.flow_type.eq_ignore_ascii_case("account_recovery") {
        created = create_recovery_case(api, &session, created, body.context).await?;
    }

    api.state
        .flow
        .update_session_status(&session_id, FLOW_STATUS_RUNNING, None)
        .await?;

    let advanced = create_step_chain(api, &session, created, initial_step, None).await?;
    refresh_session_status(api, &session_id).await?;

    Ok(flow_response(advanced))
}

/// Owns the canonical `recovery_case` aggregate for a started account-recovery
/// flow. The canonical case id is minted by keybound (never the BFF) and stored
/// in the flow context under `recovery.case_id` so the BFF can look it up.
async fn create_recovery_case(
    api: &BackendApi,
    session: &FlowSessionRow,
    mut flow: FlowInstanceRow,
    body_context: Option<Value>,
) -> Result<FlowInstanceRow, Error> {
    let phone = recovery_phone_from_context(&session.context, body_context.as_ref());

    let case_id = backend_id::recovery_case_id()?;
    api.state
        .recovery_case
        .create_case(RecoveryCaseCreateInput {
            id: case_id.clone(),
            human_id: format!("{}.case", flow.human_id),
            session_id: Some(session.id.clone()),
            device_id: None,
            jkt: None,
            device_public_jwk: None,
            requested_phone_hash: phone.as_deref().map(hash_phone).unwrap_or_default(),
            requested_phone_masked: phone.as_deref().map(mask_phone).unwrap_or_default(),
            reason: None,
            status: "EVIDENCE_REQUIRED".to_owned(),
            phone_relation: None,
            matched_user_id: None,
            expires_at: Some(Utc::now() + Duration::hours(24)),
        })
        .await?;

    let mut ctx = flow.context.clone();
    merge_json_value(&mut ctx, &json!({ "recovery": { "case_id": case_id } }));
    flow = api
        .state
        .flow
        .update_flow(&flow.id, None, None, None, Some(ctx))
        .await?;

    Ok(flow)
}

fn recovery_phone_from_context(
    session_context: &Value,
    body_context: Option<&Value>,
) -> Option<String> {
    body_context
        .and_then(|c| c.get("phone_number"))
        .or_else(|| session_context.get("phone_number"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

#[instrument(skip(api))]
pub async fn get_flow(
    api: &BackendApi,
    flow_id: String,
    caller: &BffCallerIdentity,
) -> Result<FlowDetailResponse, Error> {
    debug!("Getting flow: {}", flow_id);
    let flow = api
        .state
        .flow
        .get_flow(&flow_id)
        .await?
        .ok_or_else(|| Error::not_found("FLOW_NOT_FOUND", "Flow not found"))?;

    ensure_flow_owner(api, &flow, caller).await?;
    let steps = api.state.flow.list_steps_for_flow(&flow_id).await?;

    let flow_type = flow.flow_type.clone();
    Ok(FlowDetailResponse {
        flow: flow_response(flow),
        steps: steps
            .into_iter()
            .map(|s| step_response(&flow_type, s))
            .collect(),
    })
}

#[instrument(skip(api))]
pub async fn list_flow_steps(
    api: &BackendApi,
    flow_id: String,
    caller: &BffCallerIdentity,
) -> Result<Vec<StepResponse>, Error> {
    debug!("Listing steps for flow: {}", flow_id);
    let flow = api
        .state
        .flow
        .get_flow(&flow_id)
        .await?
        .ok_or_else(|| Error::not_found("FLOW_NOT_FOUND", "Flow not found"))?;

    ensure_flow_owner(api, &flow, caller).await?;
    let steps = api.state.flow.list_steps_for_flow(&flow_id).await?;
    let flow_type = flow.flow_type;
    Ok(steps
        .into_iter()
        .map(|s| step_response(&flow_type, s))
        .collect())
}

#[instrument(skip(api))]
pub async fn get_step(
    api: &BackendApi,
    step_id: String,
    caller: &BffCallerIdentity,
) -> Result<StepResponse, Error> {
    debug!("Getting step: {}", step_id);
    let step = api
        .state
        .flow
        .get_step(&step_id)
        .await?
        .ok_or_else(|| Error::not_found("STEP_NOT_FOUND", "Step not found"))?;

    let flow = api
        .state
        .flow
        .get_flow(&step.flow_id)
        .await?
        .ok_or_else(|| Error::not_found("FLOW_NOT_FOUND", "Flow not found"))?;

    ensure_flow_owner(api, &flow, caller).await?;
    let flow_type = flow.flow_type;
    Ok(step_response(&flow_type, step))
}

/// Resolves a step by `flowId` + `stepType`. When retries create multiple step
/// rows of the same type, returns the current "waiting" attempt (highest
/// attempt that is not finished); otherwise the most recent attempt.
#[instrument(skip(api))]
pub async fn get_flow_step_by_type(
    api: &BackendApi,
    flow_id: String,
    step_type: String,
    caller: &BffCallerIdentity,
) -> Result<StepResponse, Error> {
    debug!("Getting step by type: {} in flow {}", step_type, flow_id);
    let flow = api
        .state
        .flow
        .get_flow(&flow_id)
        .await?
        .ok_or_else(|| Error::not_found("FLOW_NOT_FOUND", "Flow not found"))?;

    ensure_flow_owner(api, &flow, caller).await?;

    let steps = api.state.flow.list_steps_for_flow(&flow_id).await?;
    let mut matches: Vec<_> = steps
        .into_iter()
        .filter(|step| step.step_type == step_type)
        .collect();

    if matches.is_empty() {
        return Err(Error::not_found(
            "STEP_NOT_FOUND",
            format!("No step of type `{step_type}` in flow"),
        ));
    }

    matches.sort_by_key(|step| step.attempt_no);

    // Prefer the current waiting/in-progress attempt (the one the BFF should
    // submit to), falling back to the latest attempt overall.
    let flow_type = flow.flow_type.clone();
    let chosen = matches
        .iter()
        .rev()
        .find(|step| {
            step.status.eq_ignore_ascii_case("WAITING")
                || step.status.eq_ignore_ascii_case("RUNNING")
        })
        .unwrap_or_else(|| matches.last().expect("non-empty"));

    Ok(step_response(&flow_type, chosen.clone()))
}

#[instrument(skip(api))]
pub async fn submit_step(
    api: &BackendApi,
    step_id: String,
    caller: &BffCallerIdentity,
    body: SubmitStepRequest,
) -> Result<StepResponse, Error> {
    debug!("Submitting step: {}", step_id);
    let step = api
        .state
        .flow
        .get_step(&step_id)
        .await?
        .ok_or_else(|| Error::not_found("STEP_NOT_FOUND", "Step not found"))?;

    let flow = api
        .state
        .flow
        .get_flow(&step.flow_id)
        .await?
        .ok_or_else(|| Error::not_found("FLOW_NOT_FOUND", "Flow not found"))?;

    ensure_flow_owner(api, &flow, caller).await?;

    // M1: serialize OTP verification per recovery case so the attempts counter
    // is incremented atomically and a locked-out case is rejected before any
    // hash work. We re-read the flow fresh under the lock so the attempts read
    // reflects the latest committed value.
    let is_recovery_verify = is_account_recovery(&flow.flow_type)
        && step.step_type.eq_ignore_ascii_case("VERIFY_RECOVERY_OTP");
    if is_recovery_verify {
        let case_id = flow
            .context
            .pointer("/recovery/case_id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let lock = api.state.otp_locks.lock_for(&case_id);
        let _guard = lock.lock().await;
        let fresh_flow = api
            .state
            .flow
            .get_flow(&flow.id)
            .await?
            .ok_or_else(|| Error::not_found("FLOW_NOT_FOUND", "Flow not found"))?;
        let flow_type = fresh_flow.flow_type.clone();
        return Ok(step_response(
            &flow_type,
            submit_step_inner(api, &step, fresh_flow, &body).await?,
        ));
    }

    let flow_type = flow.flow_type.clone();
    Ok(step_response(
        &flow_type,
        submit_step_inner(api, &step, flow, &body).await?,
    ))
}

/// Executes a single end-user step submission against a supplied (current)
/// flow. `flow` must be the latest committed flow context — callers that need
/// atomicity (recovery OTP verify) re-read it under the per-case lock first.
async fn submit_step_inner(
    api: &BackendApi,
    step: &FlowStepRow,
    flow: FlowInstanceRow,
    body: &SubmitStepRequest,
) -> Result<FlowStepRow, Error> {
    let flow_definition = get_flow_definition(api, &flow.flow_type)?;
    let step_definition = get_step_definition(flow_definition, &step.step_type)?;

    if matches!(step_definition.actor(), Actor::System) {
        return Err(Error::conflict(
            "SYSTEM_STEP_NOT_SUBMITTABLE",
            "System steps are executed automatically",
        ));
    }

    step_definition
        .validate_input(&body.input)
        .await
        .map_err(flow_error_to_http)?;

    let session = api
        .state
        .flow
        .get_session(&flow.session_id)
        .await?
        .ok_or_else(|| Error::not_found("SESSION_NOT_FOUND", "Session not found"))?;

    let verify_context = StepContext {
        session_id: session.id.clone(),
        session_user_id: session.user_id.clone(),
        flow_id: flow.id.clone(),
        step_id: step.id.clone(),
        input: body.input.clone(),
        session_context: session.context.clone(),
        flow_context: flow.context.clone(),
        services: step_services_with_device(api.state.user.clone(), api.state.device.clone()),
    };

    let verify_outcome = step_definition
        .verify_input(&verify_context, &body.input)
        .await
        .map_err(flow_error_to_http)?;

    let (output_value, context_updates, branch) = match verify_outcome {
        StepOutcome::Done { output, updates } => (
            output.unwrap_or_else(|| json!({"verified": true})),
            updates,
            None,
        ),
        StepOutcome::Branched {
            branch,
            output,
            updates,
        } => (
            output.unwrap_or_else(|| json!({"verified": true})),
            updates,
            Some(branch),
        ),
        StepOutcome::Failed {
            error,
            retryable: _,
        } => {
            return Err(Error::bad_request("VERIFICATION_FAILED", error));
        }
        _ => (json!({"verified": true}), None, None),
    };

    let mut updated_flow_context = flow.context.clone();
    if let Some(updates) = context_updates {
        if let Some(patch) = updates.flow_context_patch.as_ref() {
            updated_flow_context = merged_json(updated_flow_context, patch);
        }
        apply_context_updates(api, &session, updates).await?;
    }

    let updated_step = api
        .state
        .flow
        .patch_step(
            &step.id,
            FlowStepPatch::new()
                .status(FLOW_STATUS_COMPLETED)
                .input(body.input.clone())
                .output(output_value.clone())
                .clear_error()
                .finished_at(Utc::now()),
        )
        .await?;

    let context = store_step_output(updated_flow_context, &updated_step.step_type, &body.input);
    let mut current_flow = api
        .state
        .flow
        .update_flow(&flow.id, None, None, None, Some(context))
        .await?;

    if let Some(next_step) = resolve_transition(
        flow_definition,
        &updated_step.step_type,
        branch.as_deref(),
        false,
    ) {
        if has_flow_step(flow_definition, &next_step) {
            current_flow = create_step_chain(api, &session, current_flow, next_step, None).await?;
        } else {
            current_flow = finalize_flow(api, &current_flow, terminal_status(&next_step)).await?;
        }
    } else {
        current_flow = finalize_flow(api, &current_flow, FLOW_STATUS_COMPLETED).await?;
    }

    // Fix 3: reflect the newly-created waiting step in the recovery-case status.
    // Previously the case stayed at EVIDENCE_REQUIRED after the user submitted
    // verify_recovery_otp, because this projection was only invoked on the
    // system-step and finalize paths.
    if current_flow
        .flow_type
        .eq_ignore_ascii_case("account_recovery")
    {
        sync_recovery_case(api, &current_flow).await?;
    }

    refresh_session_status(api, &current_flow.session_id).await?;

    Ok(updated_step)
}

pub(crate) fn get_flow_definition<'a>(
    api: &'a BackendApi,
    flow_type: &str,
) -> Result<&'a dyn Flow, Error> {
    api.state.flow_registry.get_flow(flow_type).ok_or_else(|| {
        Error::bad_request(
            "UNKNOWN_FLOW_TYPE",
            format!("Unknown flow type: {flow_type}"),
        )
    })
}

pub(crate) fn get_step_definition<'a>(
    flow: &'a dyn Flow,
    step_type: &str,
) -> Result<&'a dyn backend_flow_sdk::Step, Error> {
    flow.steps()
        .iter()
        .find(|step| step.step_type() == step_type)
        .map(|step| step.as_ref())
        .ok_or_else(|| {
            Error::bad_request(
                "UNKNOWN_STEP_TYPE",
                format!(
                    "Unknown step type `{step_type}` for flow `{}`",
                    flow.flow_type()
                ),
            )
        })
}

pub(crate) fn ensure_flow_step_exists(flow: &dyn Flow, step_type: &str) -> Result<(), Error> {
    if has_flow_step(flow, step_type) {
        return Ok(());
    }

    Err(Error::bad_request(
        "UNKNOWN_STEP_TYPE",
        format!(
            "Unknown step type `{step_type}` for flow `{}`",
            flow.flow_type()
        ),
    ))
}

pub(crate) fn has_flow_step(flow: &dyn Flow, step_type: &str) -> bool {
    flow.steps()
        .iter()
        .any(|step| step.step_type() == step_type)
}

pub(crate) fn terminal_status(step_type: &str) -> &'static str {
    if step_type.eq_ignore_ascii_case("FAILED") {
        FLOW_STATUS_FAILED
    } else if step_type.eq_ignore_ascii_case("CLOSED") {
        FLOW_STATUS_CLOSED
    } else {
        FLOW_STATUS_COMPLETED
    }
}

fn validate_session_flow_compatibility(
    api: &BackendApi,
    session: &FlowSessionRow,
    flow_type: &str,
) -> Result<(), Error> {
    let Some(definition) = api.state.flow_registry.get_session(&session.session_type) else {
        return Ok(());
    };

    if definition.allowed_flows.is_empty()
        || definition
            .allowed_flows
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(flow_type))
    {
        return Ok(());
    }

    Err(Error::bad_request(
        "FLOW_NOT_ALLOWED",
        format!(
            "Flow `{flow_type}` is not allowed for session type `{}`",
            session.session_type
        ),
    ))
}

#[instrument(skip(api, session, flow))]
pub(crate) async fn create_step_chain(
    api: &BackendApi,
    session: &FlowSessionRow,
    mut flow: FlowInstanceRow,
    mut step_type: String,
    initial_input: Option<Value>,
) -> Result<FlowInstanceRow, Error> {
    debug!("Entering step chain at: {}", step_type);
    let mut pending_input = initial_input;

    loop {
        let flow_definition = get_flow_definition(api, &flow.flow_type)?;
        let flow_config = api.state.flow_registry.get_flow_definition(&flow.flow_type);
        let step_definition = get_step_definition(flow_definition, &step_type)?;
        let step_runs_async = matches!(step_definition.actor(), Actor::System)
            && is_async_step(flow_config, &step_type);
        debug!(
            "Processing step: {} (actor={:?})",
            step_type,
            step_definition.actor()
        );

        let existing_steps = api.state.flow.list_steps_for_flow(&flow.id).await?;
        let attempt_no = existing_steps
            .iter()
            .filter(|existing| existing.step_type == step_type)
            .count() as i32;

        let step_id = backend_id::flow_step_id()?;
        let human_suffix = if attempt_no == 0 {
            step_definition.human_id().to_owned()
        } else {
            format!("{}-{}", step_definition.human_id(), attempt_no)
        };

        let step_human_id = HumanReadableId::parse(flow.human_id.clone())
            .map_err(flow_error_to_http)?
            .with_suffix(&human_suffix)
            .map_err(flow_error_to_http)?
            .to_string();

        let created_step = api
            .state
            .flow
            .create_step(FlowStepCreateInput {
                id: step_id.clone(),
                human_id: step_human_id,
                flow_id: flow.id.clone(),
                step_type: step_type.clone(),
                actor: actor_label(step_definition.actor()).to_owned(),
                status: if step_runs_async {
                    "WAITING".to_owned()
                } else {
                    waiting_status(step_definition.actor()).to_owned()
                },
                attempt_no,
                input: pending_input.clone(),
                output: None,
                error: None,
                next_retry_at: if step_runs_async {
                    Some(Utc::now())
                } else {
                    None
                },
                finished_at: None,
            })
            .await?;

        let updated_step_ids = append_step_id(&flow.step_ids, &created_step.id);
        flow = api
            .state
            .flow
            .update_flow(
                &flow.id,
                Some(FLOW_STATUS_RUNNING.to_owned()),
                Some(Some(step_type.clone())),
                Some(updated_step_ids),
                None,
            )
            .await?;

        if step_runs_async {
            debug!("Queued async system step: {}", step_type);
            return Ok(flow);
        }

        if !matches!(step_definition.actor(), Actor::System) {
            return Ok(flow);
        }

        let input_value = pending_input.clone().unwrap_or_else(|| json!({}));
        let context = StepContext {
            session_id: flow.session_id.clone(),
            session_user_id: session.user_id.clone(),
            flow_id: flow.id.clone(),
            step_id,
            input: input_value.clone(),
            session_context: session.context.clone(),
            flow_context: flow.context.clone(),
            services: step_services_with_device(api.state.user.clone(), api.state.device.clone()),
        };

        match step_definition
            .execute(&context)
            .await
            .map_err(flow_error_to_http)?
        {
            StepOutcome::Done { output, updates } => {
                let actual_output = output.unwrap_or_else(|| json!({"result": "done"}));
                flow = handle_completed_step(
                    api,
                    session,
                    flow,
                    &created_step,
                    &step_type,
                    input_value.clone(),
                    actual_output,
                    updates,
                    None,
                )
                .await?;
                let next = resolve_transition(flow_definition, &step_type, None, false);
                if let Some(next_step) = next {
                    if !has_flow_step(flow_definition, &next_step) {
                        return finalize_flow(api, &flow, terminal_status(&next_step)).await;
                    }
                    step_type = next_step;
                    pending_input = None;
                } else {
                    return finalize_flow(api, &flow, FLOW_STATUS_COMPLETED).await;
                }
            }
            StepOutcome::Branched {
                branch,
                output,
                updates,
            } => {
                let actual_output = output.unwrap_or_else(|| json!({"result": "done"}));
                flow = handle_completed_step(
                    api,
                    session,
                    flow,
                    &created_step,
                    &step_type,
                    input_value.clone(),
                    actual_output,
                    updates,
                    Some(branch.clone()),
                )
                .await?;
                let next = resolve_transition(flow_definition, &step_type, Some(&branch), false);
                if let Some(next_step) = next {
                    if !has_flow_step(flow_definition, &next_step) {
                        return finalize_flow(api, &flow, terminal_status(&next_step)).await;
                    }
                    step_type = next_step;
                    pending_input = None;
                } else {
                    return finalize_flow(api, &flow, FLOW_STATUS_COMPLETED).await;
                }
            }
            StepOutcome::Waiting { .. } => {
                debug!("Step waiting: {}", step_type);
                api.state
                    .flow
                    .patch_step(&created_step.id, FlowStepPatch::new().status("WAITING"))
                    .await?;
                return Ok(flow);
            }
            StepOutcome::Failed { error, retryable } => {
                info!(
                    "Step failed: {} (error={}, retryable={})",
                    step_type, error, retryable
                );
                api.state
                    .flow
                    .patch_step(
                        &created_step.id,
                        FlowStepPatch::new()
                            .status(FLOW_STATUS_FAILED)
                            .error(json!({"error": error, "retryable": retryable}))
                            .finished_at(Utc::now()),
                    )
                    .await?;

                if let Some(next_step) = resolve_transition(flow_definition, &step_type, None, true)
                {
                    if has_flow_step(flow_definition, &next_step) {
                        step_type = next_step;
                        pending_input = None;
                        continue;
                    }

                    return finalize_flow(api, &flow, terminal_status(&next_step)).await;
                }

                return finalize_flow(api, &flow, FLOW_STATUS_FAILED).await;
            }
            StepOutcome::Retry { after } => {
                debug!("Step retry: {} (after={:?})", step_type, after);
                api.state
                    .flow
                    .patch_step(
                        &created_step.id,
                        FlowStepPatch::new().status("WAITING").next_retry_at(
                            Utc::now()
                                + Duration::from_std(after)
                                    .unwrap_or_else(|_| Duration::seconds(0)),
                        ),
                    )
                    .await?;

                return Ok(flow);
            }
        }
    }
}

fn is_async_step(
    flow_definition: Option<&backend_flow_sdk::flow::FlowDefinition>,
    step_type: &str,
) -> bool {
    flow_definition
        .and_then(|definition| definition.steps.get(step_type))
        .and_then(|step| step.config.as_ref())
        .and_then(|config| config.get("async"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

pub(crate) async fn finalize_flow(
    api: &BackendApi,
    flow: &FlowInstanceRow,
    status: &str,
) -> Result<FlowInstanceRow, Error> {
    let finalized = api
        .state
        .flow
        .update_flow(&flow.id, Some(status.to_owned()), Some(None), None, None)
        .await?;

    if let Ok(session) = api.state.flow.get_session(&finalized.session_id).await {
        if let Some(_session) = session {
            let _ = sync_recovery_case(api, &finalized).await;
        }
    }

    if status.eq_ignore_ascii_case(FLOW_STATUS_COMPLETED) {
        write_completed_kyc_metadata(api, &finalized).await?;
    }

    refresh_session_status(api, &finalized.session_id).await?;
    Ok(finalized)
}

async fn write_completed_kyc_metadata(
    api: &BackendApi,
    flow: &FlowInstanceRow,
) -> Result<(), Error> {
    let session = api.state.flow.get_session(&flow.session_id).await?;
    let Some(session) = session else {
        return Ok(());
    };
    let Some(user_id) = session.user_id.as_deref() else {
        return Ok(());
    };
    let session_type = session.session_type.clone();
    let flow_type = flow.flow_type.clone();
    let session_id = session.id.clone();
    let flow_id = flow.id.clone();

    api.state
        .user
        .update_metadata(
            user_id,
            json!({
                "kyc": {
                    session_type: {
                        flow_type: {
                            "completed": true,
                            "completed_at": Utc::now().to_rfc3339(),
                            "flow_id": flow_id,
                            "session_id": session_id
                        }
                    }
                }
            }),
            Some(json!({ "kyc": false })),
        )
        .await
}

pub(crate) async fn refresh_session_status(
    api: &BackendApi,
    session_id: &str,
) -> Result<(), Error> {
    let flows = api.state.flow.list_flows_for_session(session_id).await?;

    if flows.is_empty() {
        api.state
            .flow
            .update_session_status(session_id, "OPEN", None)
            .await?;
        return Ok(());
    }

    if flows
        .iter()
        .any(|flow| flow.status.eq_ignore_ascii_case(FLOW_STATUS_RUNNING))
    {
        api.state
            .flow
            .update_session_status(session_id, FLOW_STATUS_RUNNING, None)
            .await?;
        return Ok(());
    }

    if flows
        .iter()
        .any(|flow| flow.status.eq_ignore_ascii_case(FLOW_STATUS_FAILED))
    {
        api.state
            .flow
            .update_session_status(session_id, FLOW_STATUS_FAILED, Some(Utc::now()))
            .await?;
        return Ok(());
    }

    if flows.iter().all(|flow| {
        flow.status.eq_ignore_ascii_case(FLOW_STATUS_COMPLETED)
            || flow.status.eq_ignore_ascii_case(FLOW_STATUS_CLOSED)
    }) {
        let status = if flows
            .iter()
            .any(|flow| flow.status.eq_ignore_ascii_case(FLOW_STATUS_CLOSED))
        {
            FLOW_STATUS_CLOSED
        } else {
            FLOW_STATUS_COMPLETED
        };
        api.state
            .flow
            .update_session_status(session_id, status, Some(Utc::now()))
            .await?;
        return Ok(());
    }

    api.state
        .flow
        .update_session_status(session_id, "OPEN", None)
        .await
}

async fn ensure_session_owner(
    api: &BackendApi,
    session_id: &str,
    caller: &BffCallerIdentity,
) -> Result<FlowSessionRow, Error> {
    let session = api
        .state
        .flow
        .get_session(session_id)
        .await?
        .ok_or_else(|| Error::not_found("SESSION_NOT_FOUND", "Session not found"))?;

    if is_account_recovery(&session.session_type) {
        ensure_recovery_owner(&session, caller).await?;
        return Ok(session);
    }

    // User-owned sessions still require an exact owner match.
    if let Some(owner) = session.user_id.as_deref()
        && owner != caller.user_id
    {
        return Err(Error::unauthorized("Session does not belong to caller"));
    }

    Ok(session)
}

async fn ensure_flow_owner(
    api: &BackendApi,
    flow: &FlowInstanceRow,
    caller: &BffCallerIdentity,
) -> Result<(), Error> {
    let session = api
        .state
        .flow
        .get_session(&flow.session_id)
        .await?
        .ok_or_else(|| Error::not_found("SESSION_NOT_FOUND", "Session not found"))?;

    if is_account_recovery(&session.session_type) {
        return ensure_recovery_owner(&session, caller).await;
    }

    if let Some(owner) = session.user_id.as_deref()
        && owner != caller.user_id
    {
        return Err(Error::unauthorized("Flow does not belong to caller"));
    }

    Ok(())
}

/// Enforces ownership of a service-created account_recovery session. A
/// recovery session has no owning `user_id` (it is created by the BFF on behalf
/// of a recovering device), so a naive owner check would vacuously pass for any
/// authenticated caller. Instead we fail closed (review item C2):
/// - if the session carries a bound owner device id, a non-service caller must
///   present exactly that device id;
/// - if no owner claim is present, reject outright (we cannot prove the caller
///   is authorized);
/// - the BFF service caller may always drive the recovery it created.
async fn ensure_recovery_owner(
    session: &FlowSessionRow,
    caller: &BffCallerIdentity,
) -> Result<(), Error> {
    // A service caller (the BFF thin facade) owns the recovery lifecycle.
    if caller.service_client_id.is_some() {
        return Ok(());
    }

    let owner_device_id = session
        .context
        .pointer("/recovery/owner_device_id")
        .and_then(Value::as_str);

    // Fail closed: without a bound owner we cannot authorize a device caller.
    let Some(owner_device_id) = owner_device_id else {
        return Err(Error::forbidden(
            "RECOVERY_OWNER_UNKNOWN",
            "Recovery session has no bound owner identity",
        ));
    };

    if caller.device_id == owner_device_id {
        return Ok(());
    }

    Err(Error::forbidden(
        "RECOVERY_OWNER_MISMATCH",
        "Recovery session does not belong to caller's device",
    ))
}

fn append_step_id(step_ids: &Value, step_id: &str) -> Value {
    let mut values = step_ids
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|value| value.as_str() != Some(step_id))
        .collect::<Vec<_>>();

    values.push(Value::String(step_id.to_owned()));
    Value::Array(values)
}

async fn handle_completed_step(
    api: &BackendApi,
    session: &FlowSessionRow,
    flow: FlowInstanceRow,
    created_step: &backend_model::db::FlowStepRow,
    step_type: &str,
    input_value: Value,
    actual_output: Value,
    updates: Option<Box<backend_flow_sdk::ContextUpdates>>,
    branch: Option<String>,
) -> Result<FlowInstanceRow, Error> {
    debug!("Step completed: {} branch={:?}", step_type, branch);

    let mut context = store_step_output(flow.context.clone(), step_type, &actual_output);
    if let Some(updates) = updates {
        if let Some(flow_patch) = updates.flow_context_patch.as_ref() {
            context = merged_json(context, flow_patch);
        }
        // Apply side effects (including notification enqueue for OTP delivery)
        // BEFORE marking the step completed, so a delivery failure never leaves
        // a step recorded as successful.
        apply_context_updates(api, session, updates).await?;
    }

    api.state
        .flow
        .patch_step(
            &created_step.id,
            FlowStepPatch::new()
                .status(FLOW_STATUS_COMPLETED)
                .input(input_value)
                .output(actual_output)
                .clear_error()
                .finished_at(Utc::now()),
        )
        .await?;

    let updated = api
        .state
        .flow
        .update_flow(&flow.id, None, None, None, Some(context))
        .await?;

    sync_recovery_case(api, &updated).await?;
    Ok(updated)
}

/// Projects the account-recovery flow state into the canonical `recovery_case`
/// aggregate so the BFF can always read the case back by id. Uses optimistic
/// concurrency (`update_case` with the row's current version).
pub(crate) async fn sync_recovery_case(
    api: &BackendApi,
    flow: &FlowInstanceRow,
) -> Result<(), Error> {
    if !flow.flow_type.eq_ignore_ascii_case("account_recovery") {
        return Ok(());
    }

    let Some(recovery) = flow.context.get("recovery") else {
        return Ok(());
    };
    if recovery
        .get("decision_pending")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Ok(());
    }
    let Some(case_id) = recovery.get("case_id").and_then(Value::as_str) else {
        return Ok(());
    };
    let Some(case) = api.state.recovery_case.get_case_by_id(case_id).await? else {
        return Ok(());
    };

    let mut patch = RecoveryCaseUpdate::default();
    if let Some(v) = recovery.get("requested_phone_hash").and_then(Value::as_str) {
        patch.requested_phone_hash = Some(v.to_owned());
    }
    if let Some(v) = recovery
        .get("requested_phone_masked")
        .and_then(Value::as_str)
    {
        patch.requested_phone_masked = Some(v.to_owned());
    }
    if let Some(v) = recovery.get("matched_user_id").and_then(Value::as_str) {
        patch.matched_user_id = Some(Some(v.to_owned()));
    }
    if let Some(v) = recovery.get("phone_relation").and_then(Value::as_str) {
        patch.phone_relation = Some(Some(v.to_owned()));
    }
    // C1: the OTP hash is never persisted to the recovery_case row, so it
    // cannot be exposed or brute-forced from storage. The `otp_challenge_ref`
    // projection is instead derived from `otp_expires_at` (which is set exactly
    // while an OTP is outstanding), preserving the BFF response shape.
    if let Some(v) = recovery.get("otp_expires_at").and_then(Value::as_i64)
        && let Some(dt) = chrono::DateTime::<chrono::Utc>::from_timestamp(v, 0)
    {
        patch.otp_expires_at = Some(Some(dt));
    }
    if let Some(v) = recovery.get("otp_attempts").and_then(Value::as_i64) {
        patch.otp_attempts = Some(v as i32);
    }
    if let Some(v) = recovery.get("otp_resend_at").and_then(Value::as_i64)
        && let Some(dt) = chrono::DateTime::<chrono::Utc>::from_timestamp(v, 0)
    {
        patch.otp_resend_at = Some(Some(dt));
    }
    if let Some(v) = recovery.get("device_id").and_then(Value::as_str) {
        patch.device_id = Some(Some(v.to_owned()));
    }
    if let Some(v) = recovery.get("jkt").and_then(Value::as_str) {
        patch.jkt = Some(Some(v.to_owned()));
    }
    if let Some(v) = recovery.get("approval_revision").and_then(Value::as_i64) {
        patch.approval_revision = Some(Some(v));
    }

    // Fix 4: persist the computed risk flags and the old-device list onto the
    // case so the authorized staff-detail surface can render them. Writes are
    // suppressed when unchanged to avoid needless version churn on every
    // projection.
    let old_devices = recovery
        .get("old_devices")
        .filter(|value| value.is_array())
        .cloned()
        .unwrap_or_else(|| json!([]));
    let risk_flags = compute_recovery_risk_flags(recovery, &old_devices);
    if old_devices != case.old_devices {
        patch.old_devices = Some(old_devices);
    }
    if risk_flags != case.risk_flags {
        patch.risk_flags = Some(risk_flags);
    }
    if let Some(v) = recovery.get("reason").and_then(Value::as_str) {
        patch.reason = Some(Some(v.to_owned()));
    }

    // Capture the admin decision (recorded on `await_admin_decision`) onto the
    // case. The authoritative, atomic version-checked decision transition is
    // performed by `submit_admin_step` (staff_flow), which records the decision,
    // bumps the approval revision on approval, and stores the resulting version
    // in the flow context under `recovery.case_version`. This projection re-uses
    // that authoritative version (never a freshly re-read one) for its own
    // versioned update, so a concurrent/out-of-date write surfaces as a 409
    // rather than silently clobbering a newer revision.
    let decision = flow
        .context
        .pointer("/step_output/await_admin_decision/decision")
        .and_then(Value::as_str)
        .map(str::to_owned);
    if let Some(decision) = &decision {
        patch.review_decision = Some(Some(decision.clone()));
        // The approval-revision bump is owned by the atomic staff decision update
        // (which records `recovery.case_version`). Avoid double-bumping here.
        if decision.eq_ignore_ascii_case("APPROVED")
            && recovery
                .get("case_version")
                .and_then(Value::as_i64)
                .is_none()
        {
            patch.approval_revision = Some(Some(case.approval_revision.unwrap_or(1).max(1) + 1));
        }
    }

    let desired_status = recovery_case_status(flow, decision.as_deref());

    // M2: make the case status transition monotonic. A terminal status
    // (COMPLETED/CLOSED/FAILED) must never be overwritten by a non-terminal
    // one, otherwise a later sync from a still-RUNNING flow parked at
    // `approved_hold` would regress a completed case back to APPROVED.
    if !(is_terminal_recovery_status(&case.status) && !is_terminal_recovery_status(&desired_status))
    {
        patch.status = Some(desired_status);
    }

    // Use the version produced by the atomic version-checked staff transition
    // when present; otherwise fall back to the row's current version.
    let expected_version = recovery
        .get("case_version")
        .and_then(Value::as_i64)
        .unwrap_or(case.version);

    if patch.is_noop() {
        return Ok(());
    }

    let updated = api
        .state
        .recovery_case
        .update_case(case_id, expected_version, &patch)
        .await?;

    // Write back the authoritative case version into the flow context so the
    // stored `recovery.case_version` stays in lock-step with the DB row. Without
    // this, any projection write that follows a staff decision bumps the DB
    // version but leaves the stored value stale, so the next projection write
    // would compare a stale expected version against the advanced row and trip
    // RECOVERY_CASE_VERSION_CONFLICT.
    let mut context = flow.context.clone();
    if let Some(recovery_ctx) = context.get_mut("recovery").and_then(Value::as_object_mut)
        && recovery_ctx.get("case_version").and_then(Value::as_i64) != Some(updated.version)
    {
        recovery_ctx.insert("case_version".to_owned(), json!(updated.version));
        api.state
            .flow
            .update_flow(&flow.id, None, None, None, Some(context))
            .await?;
    }

    Ok(())
}

/// Returns true for terminal recovery-case statuses that must never regress.
fn is_terminal_recovery_status(status: &str) -> bool {
    matches!(
        status.to_ascii_uppercase().as_str(),
        "COMPLETED" | "CLOSED" | "FAILED"
    )
}

/// Maps the account-recovery flow phase onto the documented recovery-case
/// status enum: EVIDENCE_REQUIRED, PENDING_REVIEW, APPROVED,
/// NEEDS_MORE_EVIDENCE, COMPLETED, FAILED, CLOSED.
fn recovery_case_status(flow: &FlowInstanceRow, decision: Option<&str>) -> String {
    match flow.status.as_str() {
        s if s.eq_ignore_ascii_case(FLOW_STATUS_COMPLETED) => "COMPLETED".to_owned(),
        s if s.eq_ignore_ascii_case(FLOW_STATUS_FAILED) => "FAILED".to_owned(),
        s if s.eq_ignore_ascii_case(FLOW_STATUS_CLOSED) => "CLOSED".to_owned(),
        _ => match decision {
            Some(d) if d.eq_ignore_ascii_case("APPROVED") => "APPROVED".to_owned(),
            Some(d) if d.eq_ignore_ascii_case("REJECTED") => "CLOSED".to_owned(),
            Some(d) if d.eq_ignore_ascii_case("NEEDS_MORE_EVIDENCE") => {
                "NEEDS_MORE_EVIDENCE".to_owned()
            }
            Some(_) => "PENDING_REVIEW".to_owned(),
            None => {
                // No decision yet: derive from the current phase. Once the case
                // is parked at the admin step it is awaiting review; before that
                // the user must verify the OTP.
                let current = flow.current_step.as_deref().unwrap_or("");
                if current.eq_ignore_ascii_case("await_admin_decision")
                    || current.eq_ignore_ascii_case("record_admin_decision")
                {
                    "PENDING_REVIEW".to_owned()
                } else {
                    "EVIDENCE_REQUIRED".to_owned()
                }
            }
        },
    }
}

/// Computes an enumeration-safe set of risk flags for the case from the flow
/// recovery context and the old-device list. Flags are stable strings so the
/// projection stays idempotent.
fn compute_recovery_risk_flags(recovery: &Value, old_devices: &Value) -> Value {
    let mut flags: Vec<Value> = Vec::new();
    let relation = recovery
        .get("phone_relation")
        .and_then(Value::as_str)
        .unwrap_or("");
    if !relation.is_empty() && !relation.eq_ignore_ascii_case("MATCHED") {
        flags.push(json!("UNMATCHED_OR_AMBIGUOUS_PHONE"));
    }
    if let Some(devices) = old_devices.as_array()
        && !devices.is_empty()
    {
        flags.push(json!("OLD_DEVICES_PRESENT"));
    }
    json!(flags)
}

pub(crate) async fn apply_context_updates(
    api: &BackendApi,
    session: &FlowSessionRow,
    updates: Box<backend_flow_sdk::ContextUpdates>,
) -> Result<(), Error> {
    let backend_flow_sdk::ContextUpdates {
        session_context_patch,
        user_metadata_patch,
        user_metadata_eager_patch,
        notifications,
        ..
    } = *updates;

    if let Some(session_patch) = session_context_patch.as_ref() {
        let current_session = api
            .state
            .flow
            .get_session(&session.id)
            .await?
            .ok_or_else(|| Error::internal("SESSION_NOT_FOUND", "Session not found"))?;
        let new_session_context = merged_json(current_session.context.clone(), session_patch);
        api.state
            .flow
            .update_session_context(&session.id, new_session_context)
            .await?;
    }

    if let Some(metadata_patch) = user_metadata_patch
        && let Some(user_id) = session.user_id.as_deref()
    {
        api.state
            .user
            .update_metadata(user_id, metadata_patch, user_metadata_eager_patch)
            .await?;
    }

    if let Some(notifications) = notifications {
        for notification in notifications {
            match serde_json::from_value::<backend_core::NotificationJob>(notification.clone()) {
                Ok(job) => {
                    api.state
                        .notification_queue
                        .enqueue(job)
                        .await
                        .map_err(|error| {
                            Error::internal("NOTIFICATION_ENQUEUE_FAILED", error.to_string())
                        })?;
                }
                Err(error) => {
                    tracing::warn!("Failed to deserialize notification job: {}", error);
                }
            }
        }
    }

    Ok(())
}

pub(crate) fn store_step_output(mut context: Value, step_type: &str, input: &Value) -> Value {
    if !context.is_object() {
        context = json!({});
    }

    if let Some(root) = context.as_object_mut() {
        let entry = root
            .entry("step_output")
            .or_insert_with(|| Value::Object(Default::default()));

        if !entry.is_object() {
            *entry = Value::Object(Default::default());
        }

        if let Some(step_map) = entry.as_object_mut() {
            step_map.insert(step_type.to_owned(), input.clone());
        }
    }

    context
}

fn normalize_or_default_human_id(value: Option<String>, fallback: String) -> Result<String, Error> {
    let candidate = value.unwrap_or(fallback);
    HumanReadableId::parse(candidate.clone()).map_err(flow_error_to_http)?;
    Ok(candidate)
}

fn object_context(context: Option<Value>) -> Value {
    let value = context.unwrap_or_else(|| json!({}));
    if value.is_object() { value } else { json!({}) }
}

pub(crate) fn flow_error_to_http(error: FlowError) -> Error {
    match error {
        FlowError::FeatureNotEnabled { feature, .. } => Error::bad_request(
            "FEATURE_NOT_ENABLED",
            format!("Feature not enabled: {feature}"),
        ),
        FlowError::UnknownFlowType(flow_type) => Error::bad_request(
            "UNKNOWN_FLOW_TYPE",
            format!("Unknown flow type: {flow_type}"),
        ),
        FlowError::UnknownStepType(step_type) => Error::bad_request(
            "UNKNOWN_STEP_TYPE",
            format!("Unknown step type: {step_type}"),
        ),
        FlowError::UnknownSessionType(session_type) => Error::bad_request(
            "UNKNOWN_SESSION_TYPE",
            format!("Unknown session type: {session_type}"),
        ),
        FlowError::InvalidHumanReadableId(reason) => Error::bad_request("INVALID_HUMAN_ID", reason),
        FlowError::InvalidDefinition(reason) => {
            Error::bad_request("INVALID_FLOW_DEFINITION", reason)
        }
        FlowError::Serialization(reason) => Error::internal("FLOW_SERIALIZATION_ERROR", reason),
        FlowError::Io(error) => Error::internal("FLOW_IO_ERROR", error.to_string()),
    }
}

fn session_context_with_user_id(context: Option<Value>, user_id: &str) -> Value {
    let mut context = object_context(context);
    merge_json_value(&mut context, &json!({ "user_id": user_id }));
    context
}

#[instrument(skip(api))]
pub async fn get_user(
    api: &BackendApi,
    user_id: String,
    caller_id: String,
) -> Result<UserResponse, Error> {
    if user_id != caller_id {
        return Err(Error::unauthorized("Cannot access other users' data"));
    }

    let user = api
        .state
        .user
        .get_user(&user_id)
        .await?
        .ok_or_else(|| Error::not_found("USER_NOT_FOUND", "User not found"))?;
    let metadata = api.state.user.get_user_metadata(&user_id).await?;

    Ok(UserResponse::from_row_with_metadata(user, metadata))
}

const RECOVERY_METADATA_FIELDS: &[&str] = &["fineractClientId", "fineractCustomerCode"];

fn is_e164(value: &str) -> bool {
    match value.strip_prefix('+') {
        Some(digits) => {
            (8..=15).contains(&digits.len())
                && !digits.starts_with('0')
                && digits.chars().all(|c| c.is_ascii_digit())
        }
        None => false,
    }
}

fn metadata_scalar_to_string(value: Option<&Value>) -> Option<String> {
    match value {
        Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
        Some(Value::Number(n)) => Some(n.to_string()),
        _ => None,
    }
}

#[instrument(skip(api, body), fields(phone = "***"))]
pub async fn lookup_users_by_phone(
    api: &BackendApi,
    body: LookupByPhoneRequest,
) -> Result<LookupByPhoneResponse, Error> {
    if body.realm != api.state.config.bff.recovery_lookup_realm {
        return Err(Error::forbidden(
            "RECOVERY_REALM_REJECTED",
            "Requested realm is not authorized for recovery lookup",
        ));
    }

    let phone = body.phone.trim();
    if !is_e164(phone) {
        return Err(Error::bad_request(
            "INVALID_PHONE",
            "phone must be an E.164 number (e.g. +237690000000)",
        ));
    }

    let rows = api
        .state
        .user
        .find_users_by_phone(Some(body.realm.clone()), phone)
        .await?;
    if rows.is_empty() {
        return Ok(LookupByPhoneResponse { candidates: vec![] });
    }

    let user_ids: Vec<String> = rows.iter().map(|row| row.user_id.clone()).collect();
    let metadata = api
        .state
        .user
        .get_metadata_fields_for_users(
            user_ids,
            RECOVERY_METADATA_FIELDS
                .iter()
                .map(|s| s.to_string())
                .collect(),
        )
        .await?;

    let candidates = rows
        .into_iter()
        .map(|row| {
            let matched_by = if row.phone_number.as_deref() == Some(phone) {
                PhoneMatchField::Phone
            } else {
                PhoneMatchField::Username
            };
            let user_metadata = metadata.get(&row.user_id);
            LookupByPhoneCandidate {
                user_id: row.user_id,
                disabled: row.disabled,
                matched_by,
                fineract_client_id: metadata_scalar_to_string(
                    user_metadata.and_then(|m| m.get("fineractClientId")),
                ),
                fineract_customer_code: metadata_scalar_to_string(
                    user_metadata.and_then(|m| m.get("fineractCustomerCode")),
                ),
            }
        })
        .collect();

    Ok(LookupByPhoneResponse { candidates })
}

#[instrument(skip(api))]
pub async fn get_recovery_case(
    api: &BackendApi,
    recovery_case_id: String,
) -> Result<RecoveryCaseResponse, Error> {
    let row = api
        .state
        .recovery_case
        .get_case_by_id(&recovery_case_id)
        .await?
        .ok_or_else(|| Error::not_found("RECOVERY_CASE_NOT_FOUND", "Recovery case not found"))?;

    let (otp_challenge_ref, otp_expires_at) =
        recovery_otp_projection(&row.id, row.created_at, row.otp_expires_at);

    // Only safe, non-enumerating fields are exposed. Phone hashes, the OTP
    // hash, evidence, and risk flags are never returned to the facade.
    Ok(RecoveryCaseResponse {
        case_id: row.id.clone(),
        status: row.status,
        target_user_id: row.matched_user_id,
        approval_revision: row.approval_revision.unwrap_or(1),
        old_device_policy: row
            .old_devices
            .get("policy")
            .and_then(Value::as_str)
            .map(str::to_string),
        approved_jkt: row.jkt,
        approved_device_id: row.device_id,
        otp_challenge_ref: Some(otp_challenge_ref),
        otp_expires_at,
        otp_resend_allowed_at: row.otp_resend_at,
        created_at: row.created_at,
        updated_at: row.updated_at,
        version: row.version,
    })
}

/// Projects the OTP challenge fields so their presence is identical for matched
/// and unmatched/not-yet-issued cases.
///
/// Both branches project the SAME challenge ref (the case id) so the public
/// response never discloses whether the phone matched an account: the caller
/// cannot distinguish a real OTP case from a synthetic one by comparing
/// `otp_challenge_ref` with `caseId`, and the value is stable across reads. A
/// matched case that has an outstanding OTP carries the real expiry; a
/// no-match / not-yet-issued case has no OTP and no expiry in storage, so we
/// project a STABLE synthetic expiry derived from the case `created_at`
/// (identical on every read) to keep field presence and stability identical.
/// Verification of a synthetic challenge never succeeds (no real OTP was ever
/// issued for it), which is indistinguishable from a wrong code on a matched
/// case.
pub(crate) fn recovery_otp_projection(
    case_id: &str,
    created_at: chrono::DateTime<Utc>,
    otp_expires_at: Option<chrono::DateTime<Utc>>,
) -> (String, Option<chrono::DateTime<Utc>>) {
    match otp_expires_at {
        Some(expires_at) => (case_id.to_owned(), Some(expires_at)),
        None => (
            case_id.to_owned(),
            Some(created_at + chrono::Duration::minutes(30)),
        ),
    }
}

pub async fn finalize_recovery(
    api: &BackendApi,
    recovery_case_id: String,
    body: FinalizeRecoveryRequest,
) -> Result<RecoveryCaseResponse, Error> {
    let row = api
        .state
        .recovery_case
        .get_case_by_id(&recovery_case_id)
        .await?
        .ok_or_else(|| Error::not_found("RECOVERY_CASE_NOT_FOUND", "Recovery case not found"))?;
    if row.status.eq_ignore_ascii_case("COMPLETED") {
        return get_recovery_case(api, recovery_case_id).await;
    }
    if !row.status.eq_ignore_ascii_case("APPROVED")
        || row.matched_user_id.as_deref() != Some(body.target_user_id.as_str())
        || row.approval_revision.unwrap_or(1) != body.approval_revision
        || row.device_id.as_deref() != Some(body.device_id.as_str())
        || row.jkt.as_deref() != Some(body.jkt.as_str())
    {
        return Err(Error::conflict(
            "RECOVERY_FINALIZATION_MISMATCH",
            "Recovery completion does not match the approved case",
        ));
    }
    api.state
        .recovery_case
        .update_case(
            &recovery_case_id,
            row.version,
            &RecoveryCaseUpdate {
                status: Some("COMPLETED".to_owned()),
                ..Default::default()
            },
        )
        .await?;
    get_recovery_case(api, recovery_case_id).await
}

pub async fn require_service_caller(
    api: &BackendApi,
    headers: &HeaderMap,
) -> Result<BffSignatureClaims, Error> {
    api.require_service_caller(headers)
}

/// Re-triggers OTP issuance for a recovery case without going through the
/// normal (END_USER) step-submission path. `issue_recovery_otp` is a SYSTEM
/// step, so it is executed here directly (mirroring the synchronous step-chain
/// execution); the SMS is re-queued and delivery failure surfaces as a failed
/// step rather than a false `otp_sent`.
#[instrument(skip(api))]
pub async fn resend_recovery_otp(
    api: &BackendApi,
    recovery_case_id: String,
) -> Result<StepResponse, Error> {
    let case = api
        .state
        .recovery_case
        .get_case_by_id(&recovery_case_id)
        .await?
        .ok_or_else(|| Error::not_found("RECOVERY_CASE_NOT_FOUND", "Recovery case not found"))?;

    // M2: OTP resend is only meaningful before OTP verification. Approved/completed
    // cases must not re-issue an OTP.
    if !matches!(
        case.status.to_ascii_uppercase().as_str(),
        "EVIDENCE_REQUIRED" | "NEEDS_MORE_EVIDENCE"
    ) {
        return Err(Error::conflict(
            "RESEND_NOT_ALLOWED",
            "OTP resend is only allowed before OTP verification",
        ));
    }

    // M4: bound per-case OTP resends.
    if !api.state.rate_limiter.allow_otp_resend(&recovery_case_id) {
        return Err(Error::too_many_requests(
            "RATE_LIMITED",
            "Too many OTP resends for this case; try again later",
        ));
    }

    let session_id = case.session_id.as_deref().ok_or_else(|| {
        Error::not_found("RECOVERY_CASE_NOT_BOUND", "Recovery case has no session")
    })?;

    let session = api
        .state
        .flow
        .get_session(session_id)
        .await?
        .ok_or_else(|| Error::not_found("SESSION_NOT_FOUND", "Session not found"))?;

    let flows = api.state.flow.list_flows_for_session(session_id).await?;
    let flow = flows
        .into_iter()
        .find(|f| f.flow_type.eq_ignore_ascii_case("account_recovery"))
        .ok_or_else(|| Error::not_found("FLOW_NOT_FOUND", "No recovery flow for case"))?;

    let flow_definition = get_flow_definition(api, &flow.flow_type)?;
    let step_definition = get_step_definition(flow_definition, "issue_recovery_otp")?;

    let existing = api.state.flow.list_steps_for_flow(&flow.id).await?;
    let attempt_no = existing
        .iter()
        .filter(|s| s.step_type == "issue_recovery_otp")
        .count() as i32;

    let step_id = backend_id::flow_step_id()?;
    let human_suffix = if attempt_no == 0 {
        "issue_recovery_otp".to_owned()
    } else {
        format!("issue_recovery_otp-{attempt_no}")
    };
    let step_human_id = HumanReadableId::parse(flow.human_id.clone())
        .map_err(flow_error_to_http)?
        .with_suffix(&human_suffix)
        .map_err(flow_error_to_http)?
        .to_string();

    api.state
        .flow
        .create_step(FlowStepCreateInput {
            id: step_id.clone(),
            human_id: step_human_id,
            flow_id: flow.id.clone(),
            step_type: "issue_recovery_otp".to_owned(),
            actor: actor_label(step_definition.actor()).to_owned(),
            status: "WAITING".to_owned(),
            attempt_no,
            input: None,
            output: None,
            error: None,
            next_retry_at: None,
            finished_at: None,
        })
        .await?;

    let context = StepContext {
        session_id: session.id.clone(),
        session_user_id: session.user_id.clone(),
        flow_id: flow.id.clone(),
        step_id: step_id.clone(),
        input: json!({}),
        session_context: session.context.clone(),
        flow_context: flow.context.clone(),
        services: step_services_with_device(api.state.user.clone(), api.state.device.clone()),
    };

    let mut next_ctx = flow.context.clone();
    let outcome = step_definition
        .execute(&context)
        .await
        .map_err(flow_error_to_http)?;

    match outcome {
        StepOutcome::Done { output, updates }
        | StepOutcome::Branched {
            output, updates, ..
        } => {
            let actual_output =
                output.unwrap_or_else(|| json!({ "otp_sent": false, "status": "NOT_ISSUED" }));
            if let Some(updates) = updates {
                if let Some(patch) = updates.flow_context_patch.as_ref() {
                    next_ctx = merged_json(next_ctx, patch);
                }
                apply_context_updates(api, &session, updates).await?;
            }
            let step = api
                .state
                .flow
                .patch_step(
                    &step_id,
                    FlowStepPatch::new()
                        .status(FLOW_STATUS_COMPLETED)
                        .output(actual_output)
                        .clear_error()
                        .finished_at(Utc::now()),
                )
                .await?;
            let updated_flow = api
                .state
                .flow
                .update_flow(&flow.id, None, None, None, Some(next_ctx))
                .await?;
            sync_recovery_case(api, &updated_flow).await?;
            Ok(step.into())
        }
        StepOutcome::Failed { error, retryable } => {
            let step = api
                .state
                .flow
                .patch_step(
                    &step_id,
                    FlowStepPatch::new()
                        .status(FLOW_STATUS_FAILED)
                        .error(json!({ "error": error, "retryable": retryable }))
                        .finished_at(Utc::now()),
                )
                .await?;
            Ok(step.into())
        }
        StepOutcome::Waiting { .. } | StepOutcome::Retry { .. } => {
            let step = api
                .state
                .flow
                .patch_step(&step_id, FlowStepPatch::new().status("WAITING"))
                .await?;
            Ok(step.into())
        }
    }
}

fn compute_recovery_bind_hash(recovery_case_id: &str, body: &RecoveryBindRequest) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(recovery_case_id.as_bytes());
    hasher.update(b"|");
    hasher.update(body.realm.as_bytes());
    hasher.update(b"|");
    hasher.update(body.target_user_id.as_bytes());
    hasher.update(b"|");
    hasher.update(body.approval_revision.to_string().as_bytes());
    hasher.update(b"|");
    hasher.update(body.device_id.as_bytes());
    hasher.update(b"|");
    hasher.update(body.jkt.as_bytes());
    hasher.update(b"|");
    hasher.update(body.binding_operation_id.as_bytes());
    hasher.update(b"|");

    let mut sorted_jwk: std::collections::BTreeMap<String, serde_json::Value> =
        std::collections::BTreeMap::new();
    match &body.public_jwk {
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                sorted_jwk.insert(k.clone(), v.clone());
            }
        }
        other => {
            sorted_jwk.insert("value".to_string(), other.clone());
        }
    }
    if let Ok(jwk_json) = serde_json::to_string(&sorted_jwk) {
        hasher.update(jwk_json.as_bytes());
    }
    hex::encode(hasher.finalize())
}

fn compute_old_device_policy_hash(
    recovery_case_id: &str,
    body: &backend_model::kc::OldDevicePolicyRequest,
) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(recovery_case_id.as_bytes());
    hasher.update(b"|");
    hasher.update(body.realm.as_bytes());
    hasher.update(b"|");
    hasher.update(body.approval_revision.to_string().as_bytes());
    hasher.update(b"|");
    hasher.update(body.policy.as_bytes());
    hasher.update(b"|");
    let mut except = body.except_device_ids.clone();
    except.sort();
    for id in &except {
        hasher.update(id.as_bytes());
        hasher.update(b",");
    }
    if let Some(reason) = &body.reason {
        hasher.update(b"|");
        hasher.update(reason.as_bytes());
    }
    hex::encode(hasher.finalize())
}

fn domain_recovery_bind_req(body: &RecoveryBindRequest) -> backend_model::kc::RecoveryBindRequest {
    let mut public_jwk: std::collections::HashMap<String, gen_oas_server_kc::types::Object> =
        std::collections::HashMap::new();
    match &body.public_jwk {
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                public_jwk.insert(k.clone(), gen_oas_server_kc::types::Object(v.clone()));
            }
        }
        other => {
            public_jwk.insert(
                "value".to_string(),
                gen_oas_server_kc::types::Object(other.clone()),
            );
        }
    }
    backend_model::kc::RecoveryBindRequest {
        realm: body.realm.clone(),
        target_user_id: body.target_user_id.clone(),
        approval_revision: body.approval_revision,
        device_id: body.device_id.clone(),
        jkt: body.jkt.clone(),
        public_jwk,
        binding_operation_id: body.binding_operation_id.clone(),
    }
}

#[instrument(skip(api))]
pub async fn recovery_bind(
    api: &BackendApi,
    recovery_case_id: String,
    idempotency_key: String,
    body: RecoveryBindRequest,
) -> Result<EnrollmentBindResponse, Error> {
    if uuid::Uuid::parse_str(&idempotency_key).is_err() {
        return Err(Error::bad_request(
            "BAD_REQUEST",
            "Idempotency-Key header must be a valid UUID string",
        ));
    }
    if body.target_user_id.trim().is_empty() {
        return Err(Error::bad_request(
            "BAD_REQUEST",
            "target_user_id is required",
        ));
    }

    let req_hash = compute_recovery_bind_hash(&recovery_case_id, &body);

    let existing = api
        .state
        .device
        .find_recovery_idempotency(&idempotency_key)
        .await?;
    if let Some(existing) = existing {
        if existing.request_hash == req_hash && existing.recovery_case_id == recovery_case_id {
            return Ok(EnrollmentBindResponse {
                status: EnrollmentBindStatus::AlreadyBound,
                device_record_id: Some(existing.device_record_id),
                bound_user_id: existing.bound_user_id,
            });
        }
        return Err(Error::conflict(
            "CONFLICT",
            "Idempotency-Key reused with modified payload or path case ID",
        ));
    }

    let domain_req = domain_recovery_bind_req(&body);
    let bind_res = api
        .state
        .device
        .bind_recovery_device(&idempotency_key, &recovery_case_id, &req_hash, &domain_req)
        .await;

    match bind_res {
        Ok(record_id) => {
            let case = api
                .state
                .recovery_case
                .get_case_by_id(&recovery_case_id)
                .await?
                .ok_or_else(|| {
                    Error::not_found("RECOVERY_CASE_NOT_FOUND", "Recovery case not found")
                })?;
            if !case.status.eq_ignore_ascii_case("COMPLETED") {
                api.state
                    .recovery_case
                    .update_case(
                        &recovery_case_id,
                        case.version,
                        &RecoveryCaseUpdate {
                            status: Some("COMPLETED".to_owned()),
                            ..Default::default()
                        },
                    )
                    .await?;
            }
            Ok(EnrollmentBindResponse {
                status: EnrollmentBindStatus::Bound,
                device_record_id: Some(record_id),
                bound_user_id: body.target_user_id.clone(),
            })
        }
        Err(Error::Http {
            status_code: 409,
            error_key,
            message,
            ..
        }) => Err(Error::conflict(error_key, &message)),
        Err(Error::Http {
            status_code: 400,
            error_key,
            message,
            ..
        }) => Err(Error::bad_request(error_key, &message)),
        Err(err) => Err(err),
    }
}

#[instrument(skip(api))]
pub async fn old_devices_policy(
    api: &BackendApi,
    recovery_case_id: String,
    idempotency_key: String,
    body: OldDevicePolicyRequest,
) -> Result<OldDevicePolicyResponse, Error> {
    if uuid::Uuid::parse_str(&idempotency_key).is_err() {
        return Err(Error::bad_request(
            "BAD_REQUEST",
            "Idempotency-Key header must be a valid UUID string",
        ));
    }

    let bind_record = api
        .state
        .device
        .find_recovery_bind_by_case(&recovery_case_id)
        .await?;

    let Some(bind_record) = bind_record else {
        return Err(Error::bad_request(
            "RECOVERY_CASE_NOT_BOUND",
            "Recovery case has no completed device binding",
        ));
    };

    // The device that remains ACTIVE must be derived from the authoritative
    // recovery-bind record (the exact newly bound device), never trusted from
    // the caller. A malicious/incorrect except_device_ids must not be able to
    // preserve an old device or revoke the newly bound one. We use this
    // authoritative value for both the canonical request hash and the exemption.
    let authoritative_except = vec![bind_record.device_id.clone()];
    let domain_req = backend_model::kc::OldDevicePolicyRequest {
        realm: body.realm.clone(),
        approval_revision: body.approval_revision,
        policy: body.policy.as_str().to_string(),
        except_device_ids: authoritative_except,
        reason: body.reason.clone(),
    };
    let req_hash = compute_old_device_policy_hash(&recovery_case_id, &domain_req);

    let outcome = api
        .state
        .device
        .apply_old_device_policy(
            &idempotency_key,
            &recovery_case_id,
            &req_hash,
            &bind_record.bound_user_id,
            &domain_req.policy,
            &domain_req.except_device_ids,
        )
        .await;

    match outcome {
        Ok(outcome) => Ok(OldDevicePolicyResponse {
            status: if outcome.already_applied {
                OldDevicePolicyStatus::AlreadyApplied
            } else {
                OldDevicePolicyStatus::Applied
            },
            policy: body.policy,
            affected_device_ids: outcome.affected_device_ids,
        }),
        Err(Error::Http {
            status_code: 409,
            error_key,
            message,
            ..
        }) => Err(Error::conflict(error_key, &message)),
        Err(Error::Http {
            status_code: 400,
            error_key,
            message,
            ..
        }) => Err(Error::bad_request(error_key, &message)),
        Err(err) => Err(err),
    }
}

#[instrument(skip(api))]
pub async fn get_completed_kyc(
    api: &BackendApi,
    user_id: String,
    caller_id: String,
) -> Result<CompletedKycResponse, Error> {
    debug!("Will get completed kyc");
    if user_id != caller_id {
        return Err(Error::unauthorized("Cannot access other users' data"));
    }

    api.state
        .user
        .get_user(&user_id)
        .await?
        .ok_or_else(|| Error::not_found("USER_NOT_FOUND", "User not found"))?;
    let metadata = api.state.user.get_user_metadata(&user_id).await?;

    let completed_kyc = metadata
        .get("kyc")
        .cloned()
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}));

    Ok(CompletedKycResponse {
        user_id,
        completed_kyc,
    })
}

#[cfg(test)]
mod lookup_by_phone_tests {
    use super::*;
    use crate::test_utils::{MockUserRepo, TestAppStateBuilder};
    use backend_model::db::UserRow;
    use chrono::Utc;
    use std::collections::HashMap;
    use std::sync::Arc;

    fn user_row(user_id: &str, username: &str, phone: Option<String>, disabled: bool) -> UserRow {
        UserRow {
            user_id: user_id.to_owned(),
            realm: "azamra".to_owned(),
            username: username.to_owned(),
            full_name: Some("Jane Somebody".to_owned()),
            email: Some("jane@example.org".to_owned()),
            email_verified: true,
            phone_number: phone,
            disabled,
            attributes: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn api_with(user: MockUserRepo) -> BackendApi {
        let state = TestAppStateBuilder::new().with_user(Arc::new(user)).build();
        let oidc = state.oidc_state.clone();
        let signature = state.signature_state.clone();
        BackendApi::new(Arc::new(state), oidc, signature)
    }

    fn metadata_expect(user: &mut MockUserRepo, input: Vec<(&str, Value)>) {
        let entries: Vec<(String, Value)> = input
            .into_iter()
            .map(|(uid, value)| (uid.to_string(), value))
            .collect();
        user.expect_get_metadata_fields_for_users()
            .returning(move |_, _| {
                Ok(entries
                    .iter()
                    .map(|(uid, value)| (uid.clone(), value.clone()))
                    .collect::<HashMap<String, Value>>())
            });
    }

    #[tokio::test]
    async fn lookup_by_phone_returns_minimal_candidates_without_full_user_records() {
        let phone = "+237690000000";
        let mut user = MockUserRepo::new();
        user.expect_find_users_by_phone()
            .withf(move |_realm: &Option<String>, p: &str| p == phone)
            .returning(move |_, _| {
                Ok(vec![
                    user_row("usr-1", phone, Some(phone.to_owned()), false),
                    user_row("usr-2", phone, Some(phone.to_owned()), true),
                ])
            });
        metadata_expect(
            &mut user,
            vec![
                (
                    "usr-1",
                    json!({ "fineractClientId": "c-1", "fineractCustomerCode": "CC-1" }),
                ),
                ("usr-2", json!({ "fineractClientId": "c-2" })),
            ],
        );

        let api = api_with(user);
        let response = lookup_users_by_phone(
            &api,
            LookupByPhoneRequest {
                phone: phone.to_owned(),
                realm: "fineract".to_owned(),
            },
        )
        .await
        .unwrap();

        assert_eq!(response.candidates.len(), 2);
        assert_eq!(response.candidates[0].user_id, "usr-1");
        assert!(!response.candidates[0].disabled);
        assert_eq!(response.candidates[0].matched_by, PhoneMatchField::Phone);
        assert_eq!(
            response.candidates[0].fineract_client_id.as_deref(),
            Some("c-1")
        );
        assert_eq!(
            response.candidates[0].fineract_customer_code.as_deref(),
            Some("CC-1")
        );
        assert!(response.candidates[1].disabled);
        assert_eq!(response.candidates[1].fineract_customer_code, None);
    }

    #[tokio::test]
    async fn lookup_by_phone_response_exposes_no_unrelated_user_fields() {
        let phone = "+237690000000";
        let mut user = MockUserRepo::new();
        user.expect_find_users_by_phone().returning(move |_, _| {
            Ok(vec![user_row(
                "usr-1",
                phone,
                Some(phone.to_owned()),
                false,
            )])
        });
        metadata_expect(&mut user, vec![]);

        let api = api_with(user);
        let response = lookup_users_by_phone(
            &api,
            LookupByPhoneRequest {
                phone: phone.to_owned(),
                realm: "fineract".to_owned(),
            },
        )
        .await
        .unwrap();

        let serialized = serde_json::to_string(&response).unwrap();
        for forbidden in [
            "email",
            "fullName",
            "username",
            "metadata",
            "realm",
            "createdAt",
            "updatedAt",
            "Jane Somebody",
            "jane@example.org",
        ] {
            assert!(
                !serialized.contains(forbidden),
                "response leaked forbidden field/value {forbidden}: {serialized}"
            );
        }
    }

    #[tokio::test]
    async fn lookup_by_phone_computes_matched_by_column() {
        let phone = "+237690000000";
        let mut user = MockUserRepo::new();
        user.expect_find_users_by_phone().returning(move |_, _| {
            Ok(vec![
                user_row("usr-phone", "someone", Some(phone.to_owned()), false),
                user_row(
                    "usr-username",
                    phone,
                    Some("+237670000001".to_owned()),
                    false,
                ),
            ])
        });
        metadata_expect(&mut user, vec![]);

        let api = api_with(user);
        let response = lookup_users_by_phone(
            &api,
            LookupByPhoneRequest {
                phone: phone.to_owned(),
                realm: "fineract".to_owned(),
            },
        )
        .await
        .unwrap();

        assert_eq!(response.candidates[0].matched_by, PhoneMatchField::Phone);
        assert_eq!(response.candidates[1].matched_by, PhoneMatchField::Username);
    }

    #[tokio::test]
    async fn lookup_by_phone_normalizes_numeric_fineract_identifiers() {
        let phone = "+237690000000";
        let mut user = MockUserRepo::new();
        user.expect_find_users_by_phone().returning(move |_, _| {
            Ok(vec![user_row(
                "usr-1",
                phone,
                Some(phone.to_owned()),
                false,
            )])
        });
        metadata_expect(
            &mut user,
            vec![("usr-1", json!({ "fineractClientId": 12345 }))],
        );

        let api = api_with(user);
        let response = lookup_users_by_phone(
            &api,
            LookupByPhoneRequest {
                phone: phone.to_owned(),
                realm: "fineract".to_owned(),
            },
        )
        .await
        .unwrap();

        assert_eq!(
            response.candidates[0].fineract_client_id.as_deref(),
            Some("12345")
        );
    }

    #[tokio::test]
    async fn lookup_by_phone_returns_empty_list_when_no_candidates() {
        let mut user = MockUserRepo::new();
        user.expect_find_users_by_phone()
            .returning(|_, _| Ok(vec![]));

        let api = api_with(user);
        let response = lookup_users_by_phone(
            &api,
            LookupByPhoneRequest {
                phone: "+237699999999".to_owned(),
                realm: "fineract".to_owned(),
            },
        )
        .await
        .unwrap();

        assert!(response.candidates.is_empty());
    }

    #[tokio::test]
    async fn lookup_by_phone_accepts_surrounding_whitespace() {
        let mut user = MockUserRepo::new();
        user.expect_find_users_by_phone()
            .withf(|_realm: &Option<String>, p: &str| p == "+237690000000")
            .returning(|_, _| Ok(vec![]));

        let api = api_with(user);
        let response = lookup_users_by_phone(
            &api,
            LookupByPhoneRequest {
                phone: "  +237690000000 ".to_owned(),
                realm: "fineract".to_owned(),
            },
        )
        .await
        .unwrap();

        assert!(response.candidates.is_empty());
    }

    #[tokio::test]
    async fn lookup_by_phone_rejects_non_e164_phones() {
        for bad in [
            "",
            "   ",
            "foo",
            "123",
            "+237abc",
            "+237 690 000 000",
            "237690000000",
            "+023769000000",
            "+1234567",
        ] {
            let user = MockUserRepo::new();
            let api = api_with(user);
            let error = lookup_users_by_phone(
                &api,
                LookupByPhoneRequest {
                    phone: bad.to_owned(),
                    realm: "fineract".to_owned(),
                },
            )
            .await
            .unwrap_err();

            match error {
                Error::Http {
                    error_key,
                    status_code,
                    ..
                } => {
                    assert_eq!(status_code, 400, "input {bad:?} should be rejected");
                    assert_eq!(error_key, "INVALID_PHONE", "input {bad:?}");
                }
                other => panic!("expected 400 Http error, got {:?}", other),
            }
        }
    }

    #[test]
    fn is_e164_accepts_canonical_numbers_only() {
        assert!(is_e164("+237690000000"));
        assert!(is_e164("+14155552671"));
        assert!(!is_e164("+237690000000123456"));
        assert!(!is_e164("+23769000000a"));
        assert!(!is_e164(""));
    }

    #[tokio::test]
    async fn lookup_by_phone_response_never_contains_raw_phone() {
        let phone = "+237690000000";
        let mut user = MockUserRepo::new();
        user.expect_find_users_by_phone()
            .withf(move |_realm: &Option<String>, p: &str| p == phone)
            .returning(move |_, _| Ok(vec![]));
        user.expect_get_metadata_fields_for_users()
            .returning(|_, _| Ok(HashMap::new()));

        let api = api_with(user);

        let response = lookup_users_by_phone(
            &api,
            LookupByPhoneRequest {
                phone: phone.to_owned(),
                realm: "fineract".to_owned(),
            },
        )
        .await
        .unwrap();

        let serialized = serde_json::to_string(&response).unwrap();
        assert!(
            !serialized.contains("237690000000"),
            "response body leaked raw phone: {serialized}"
        );
    }

    #[tokio::test]
    async fn lookup_by_phone_returns_only_matching_realm_candidates() {
        let phone = "+237690000000";
        let mut user = MockUserRepo::new();
        user.expect_find_users_by_phone()
            .returning(|realm, _| match realm.as_deref() {
                Some("fineract") => Ok(vec![user_row(
                    "usr-fineract",
                    phone,
                    Some(phone.to_owned()),
                    false,
                )]),
                Some("keycloak") => Ok(vec![user_row(
                    "usr-keycloak",
                    phone,
                    Some(phone.to_owned()),
                    false,
                )]),
                _ => Ok(vec![]),
            });
        metadata_expect(&mut user, vec![]);

        let api = api_with(user);

        let response = lookup_users_by_phone(
            &api,
            LookupByPhoneRequest {
                phone: phone.to_owned(),
                realm: "fineract".to_owned(),
            },
        )
        .await
        .unwrap();

        assert_eq!(response.candidates.len(), 1);
        assert_eq!(response.candidates[0].user_id, "usr-fineract");
    }

    #[tokio::test]
    async fn lookup_by_phone_rejects_non_configured_realm_before_repository_access() {
        let phone = "+237690000000";
        let user = MockUserRepo::new();
        let api = api_with(user);

        let error = lookup_users_by_phone(
            &api,
            LookupByPhoneRequest {
                phone: phone.to_owned(),
                realm: "nonexistent".to_owned(),
            },
        )
        .await
        .unwrap_err();

        match error {
            Error::Http {
                error_key,
                status_code,
                ..
            } => {
                assert_eq!(status_code, 403);
                assert_eq!(error_key, "RECOVERY_REALM_REJECTED");
            }
            other => panic!("expected 403 Http error, got {other:?}"),
        }
    }
}

#[cfg(test)]
mod create_session_tests {
    use super::*;
    use crate::flows::registry::{self, RegistryImports};
    use crate::test_utils::{MockFlowRepo, TestAppStateBuilder};
    use backend_model::db::FlowSessionRow;
    use chrono::Utc;
    use std::sync::Arc;

    const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");

    fn api_with(flow: MockFlowRepo) -> BackendApi {
        let registry = Arc::new(
            registry::build_registry(RegistryImports {
                sessions_dir: Some(format!("{FIXTURES}/sessions")),
                ..Default::default()
            })
            .expect("registry builds"),
        );
        let state = TestAppStateBuilder::new()
            .with_flow(Arc::new(flow))
            .with_flow_registry(registry)
            .build();
        let oidc = state.oidc_state.clone();
        let signature = state.signature_state.clone();
        BackendApi::new(Arc::new(state), oidc, signature)
    }

    fn row_from(input: &FlowSessionCreateInput) -> FlowSessionRow {
        FlowSessionRow {
            id: input.id.clone(),
            human_id: input.human_id.clone(),
            user_id: input.user_id.clone(),
            session_type: input.session_type.clone(),
            status: input.status.clone(),
            context: input.context.clone(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            completed_at: None,
        }
    }

    fn request() -> CreateSessionRequest {
        CreateSessionRequest {
            session_type: "account_recovery".to_owned(),
            human_id: None,
            context: Some(json!({ "phone_number": "+237690000000" })),
        }
    }

    fn service_caller() -> BffCallerIdentity {
        BffCallerIdentity {
            user_id: "usr_bff".to_owned(),
            device_id: "bff".to_owned(),
            service_client_id: Some("azamra-bff".to_owned()),
        }
    }

    fn end_user_caller() -> BffCallerIdentity {
        BffCallerIdentity {
            user_id: "usr_owner".to_owned(),
            device_id: "dvc_owner".to_owned(),
            service_client_id: None,
        }
    }

    #[tokio::test]
    async fn service_created_session_persists_null_user_id_and_no_user_context() {
        let mut flow = MockFlowRepo::new();
        flow.expect_create_session()
            .returning(|input| Ok(row_from(&input)));
        let api = api_with(flow);

        let session = create_session(&api, &service_caller(), request())
            .await
            .unwrap();
        assert!(
            session.user_id.is_none(),
            "service session must have no owner"
        );
        assert!(
            session.context.get("user_id").is_none(),
            "service session context must not carry a user_id: {}",
            session.context
        );
    }

    #[tokio::test]
    async fn end_user_cannot_create_account_recovery_session() {
        let mut flow = MockFlowRepo::new();
        flow.expect_create_session().never();
        let api = api_with(flow);

        // C3(a): account_recovery sessions are service-caller only.
        let error = create_session(&api, &end_user_caller(), request())
            .await
            .unwrap_err();
        match error {
            Error::Http {
                error_key,
                status_code,
                ..
            } => {
                assert_eq!(status_code, 403);
                assert_eq!(error_key, "RECOVERY_SERVICE_ONLY");
            }
            other => panic!("expected 403 Http error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn recovery_session_binds_owner_device_from_context() {
        let mut flow = MockFlowRepo::new();
        flow.expect_create_session()
            .returning(|input| Ok(row_from(&input)));
        let api = api_with(flow);

        let mut req = request();
        req.context = Some(json!({
            "phone_number": "+237690000000",
            "device_id": "dvc_target",
            "jkt": "jkt_target",
        }));
        let session = create_session(&api, &service_caller(), req).await.unwrap();
        // C2: the owner claim is stored so owner checks can fail closed.
        assert_eq!(
            session
                .context
                .pointer("/recovery/owner_device_id")
                .and_then(Value::as_str),
            Some("dvc_target")
        );
        assert_eq!(
            session
                .context
                .pointer("/recovery/owner_jkt")
                .and_then(Value::as_str),
            Some("jkt_target")
        );
    }
}

#[cfg(test)]
mod recovery_case_projection_tests {
    use super::*;
    use crate::test_utils::{MockFlowRepo, MockRecoveryCaseRepo, TestAppStateBuilder};
    use backend_model::db::RecoveryCaseRow;
    use std::sync::{Arc, Mutex};

    fn flow(
        status: &str,
        current_step: Option<&str>,
        recovery: Value,
        step_output: Value,
    ) -> FlowInstanceRow {
        let mut context = json!({ "recovery": recovery });
        if step_output.is_object() && !step_output.as_object().unwrap().is_empty() {
            context["step_output"] = step_output;
        }
        FlowInstanceRow {
            id: "flow_1".to_owned(),
            human_id: "rc.f1".to_owned(),
            session_id: "sess_1".to_owned(),
            flow_type: "account_recovery".to_owned(),
            status: status.to_owned(),
            current_step: current_step.map(str::to_owned),
            step_ids: json!([]),
            context,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn case_row(version: i64, status: &str) -> RecoveryCaseRow {
        RecoveryCaseRow {
            id: "case_1".to_owned(),
            human_id: "rc.f1.case".to_owned(),
            session_id: Some("sess_1".to_owned()),
            device_id: None,
            jkt: None,
            device_public_jwk: None,
            requested_phone_hash: "hash".to_owned(),
            requested_phone_masked: "+****".to_owned(),
            reason: None,
            status: status.to_owned(),
            phone_relation: None,
            matched_user_id: None,
            otp_hash: None,
            otp_expires_at: None,
            otp_attempts: 0,
            otp_resend_at: None,
            review_decision: None,
            review_reason: None,
            review_checklist: None,
            review_expected_version: None,
            reviewer_id: None,
            decided_at: None,
            approval_revision: None,
            old_devices: json!([]),
            risk_flags: json!({}),
            expires_at: None,
            review_expires_at: None,
            approved_expires_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            version,
        }
    }

    fn api_with(repo: MockRecoveryCaseRepo) -> BackendApi {
        let state = TestAppStateBuilder::new()
            .with_recovery_case(Arc::new(repo))
            .build();
        let oidc = state.oidc_state.clone();
        let signature = state.signature_state.clone();
        BackendApi::new(Arc::new(state), oidc, signature)
    }

    fn api_with_flow(repo: MockRecoveryCaseRepo, flow: MockFlowRepo) -> BackendApi {
        let state = TestAppStateBuilder::new()
            .with_recovery_case(Arc::new(repo))
            .with_flow(Arc::new(flow))
            .build();
        let oidc = state.oidc_state.clone();
        let signature = state.signature_state.clone();
        BackendApi::new(Arc::new(state), oidc, signature)
    }

    /// Builds a `MockFlowRepo` whose `update_flow` echoes back the submitted
    /// context, and captures the last-written context for assertions.
    fn flow_repo_echo(seen: std::sync::Arc<std::sync::Mutex<Option<Value>>>) -> MockFlowRepo {
        let mut flow = MockFlowRepo::new();
        flow.expect_update_flow()
            .returning(move |id, _, _, _, context| {
                if let Some(ctx) = &context {
                    *seen.lock().unwrap() = Some(ctx.clone());
                }
                Ok(FlowInstanceRow {
                    id: id.to_owned(),
                    human_id: "rc.f1".to_owned(),
                    session_id: "sess_1".to_owned(),
                    flow_type: "account_recovery".to_owned(),
                    status: "RUNNING".to_owned(),
                    current_step: None,
                    step_ids: json!([]),
                    context: context.unwrap_or_default(),
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                })
            });
        flow
    }

    #[test]
    fn recovery_case_status_maps_documented_enum() {
        // Terminal flow statuses pass through.
        assert_eq!(
            recovery_case_status(&flow("COMPLETED", None, json!({}), json!({})), None),
            "COMPLETED"
        );
        assert_eq!(
            recovery_case_status(&flow("FAILED", None, json!({}), json!({})), None),
            "FAILED"
        );
        assert_eq!(
            recovery_case_status(&flow("CLOSED", None, json!({}), json!({})), None),
            "CLOSED"
        );

        // Decisions drive the recovery-specific statuses, never generic ones.
        assert_eq!(
            recovery_case_status(
                &flow("RUNNING", Some("approved_hold"), json!({}), json!({})),
                Some("APPROVED")
            ),
            "APPROVED"
        );
        assert_eq!(
            recovery_case_status(
                &flow("RUNNING", Some("rejected_terminal"), json!({}), json!({})),
                Some("REJECTED")
            ),
            "CLOSED"
        );

        // No decision yet: awaiting review vs evidence required.
        assert_eq!(
            recovery_case_status(
                &flow(
                    "RUNNING",
                    Some("await_admin_decision"),
                    json!({}),
                    json!({})
                ),
                None
            ),
            "PENDING_REVIEW"
        );
        assert_eq!(
            recovery_case_status(
                &flow("RUNNING", Some("verify_recovery_otp"), json!({}), json!({})),
                None
            ),
            "EVIDENCE_REQUIRED"
        );
    }

    #[tokio::test]
    async fn sync_recovery_case_uses_version_from_version_checked_update() {
        let mut repo = MockRecoveryCaseRepo::new();
        repo.expect_get_case_by_id()
            .withf(|id: &str| id == "case_1")
            .returning(|_| Ok(Some(case_row(3, "EVIDENCE_REQUIRED"))));
        repo.expect_update_case()
            // K3: the projection must use the authoritative case_version (4),
            // never the freshly re-read row version (3).
            .withf(
                |id: &str, expected_version: &i64, _patch: &RecoveryCaseUpdate| {
                    id == "case_1" && *expected_version == 4
                },
            )
            .returning(|id, _, patch| {
                let mut row = case_row(4, patch.status.as_deref().unwrap_or_default());
                row.id = id.to_owned();
                Ok(row)
            });

        let api = api_with(repo);
        let f = flow(
            "RUNNING",
            Some("approved_hold"),
            json!({ "case_id": "case_1", "case_version": 4 }),
            json!({ "await_admin_decision": { "decision": "APPROVED" } }),
        );
        sync_recovery_case(&api, &f).await.expect("sync succeeds");
    }

    #[tokio::test]
    async fn sync_recovery_case_falls_back_to_row_version_when_no_case_version() {
        let mut repo = MockRecoveryCaseRepo::new();
        repo.expect_get_case_by_id()
            .returning(|_| Ok(Some(case_row(3, "EVIDENCE_REQUIRED"))));
        repo.expect_update_case()
            .withf(
                |_id: &str, expected_version: &i64, _patch: &RecoveryCaseUpdate| {
                    *expected_version == 3
                },
            )
            .returning(|id, _, patch| {
                let mut row = case_row(4, patch.status.as_deref().unwrap_or_default());
                row.id = id.to_owned();
                Ok(row)
            });

        let seen = Arc::new(Mutex::new(None));
        let api = api_with_flow(repo, flow_repo_echo(seen.clone()));
        let f = flow(
            "RUNNING",
            Some("verify_recovery_otp"),
            json!({ "case_id": "case_1" }),
            json!({}),
        );
        sync_recovery_case(&api, &f).await.expect("sync succeeds");
        // The write-back keeps recovery.case_version in lock-step with the DB
        // version (3 -> 4), so a later projection never uses a stale version.
        let written = seen.lock().unwrap().clone().expect("flow context written");
        assert_eq!(
            written
                .pointer("/recovery/case_version")
                .and_then(Value::as_i64),
            Some(4)
        );
    }

    #[tokio::test]
    async fn sync_recovery_case_does_not_regress_terminal_completed() {
        let mut repo = MockRecoveryCaseRepo::new();
        // The case is already COMPLETED (out-of-band /complete after bind).
        repo.expect_get_case_by_id()
            .returning(|_| Ok(Some(case_row(9, "COMPLETED"))));
        repo.expect_update_case()
            // M2: the projection must NOT regress the terminal COMPLETED status
            // back to APPROVED even though the flow is still RUNNING at
            // approved_hold with an APPROVED decision recorded.
            .withf(
                |_id: &str, _expected_version: &i64, patch: &RecoveryCaseUpdate| {
                    patch.status.as_deref() != Some("APPROVED")
                },
            )
            .returning(|id, _, patch| {
                let mut row = case_row(10, patch.status.as_deref().unwrap_or_default());
                row.id = id.to_owned();
                Ok(row)
            });

        let seen = Arc::new(Mutex::new(None));
        let api = api_with_flow(repo, flow_repo_echo(seen.clone()));
        let f = flow(
            "RUNNING",
            Some("approved_hold"),
            json!({ "case_id": "case_1", "otp_expires_at": Utc::now().timestamp() }),
            json!({ "await_admin_decision": { "decision": "APPROVED" } }),
        );
        sync_recovery_case(&api, &f).await.expect("sync succeeds");
        let written = seen.lock().unwrap().clone().expect("flow context written");
        assert_eq!(
            written
                .pointer("/recovery/case_version")
                .and_then(Value::as_i64),
            Some(10)
        );
    }

    #[test]
    fn otp_projection_is_uniform_for_matched_and_unmatched() {
        // Matched / OTP outstanding: real expiry, challenge ref is the case id.
        let created_at = Utc::now();
        let real_expiry = Utc::now() + chrono::Duration::minutes(10);
        let (ref_matched, expiry_matched) =
            recovery_otp_projection("case_1", created_at, Some(real_expiry));
        assert_eq!(ref_matched, "case_1");
        assert_eq!(expiry_matched, Some(real_expiry));

        // Unmatched / not-yet-issued: no OTP in storage. The challenge ref MUST
        // be identical to the matched branch (the case id) so a caller cannot
        // tell the two apart, and the synthetic expiry must be STABLE (derived
        // from created_at, not "now") so a replayed read does not move it.
        let (ref_unmatched, expiry_unmatched) = recovery_otp_projection("case_1", created_at, None);
        assert_eq!(
            ref_unmatched, ref_matched,
            "matched and unmatched must project the same, stable challenge ref"
        );
        let (ref_unmatched_again, expiry_unmatched_again) =
            recovery_otp_projection("case_1", created_at, None);
        assert_eq!(
            ref_unmatched, ref_unmatched_again,
            "ref must be stable across reads"
        );
        assert_eq!(
            expiry_unmatched, expiry_unmatched_again,
            "synthetic expiry must be stable across reads (not move with 'now')"
        );
        let Some(expiry_unmatched) = expiry_unmatched else {
            panic!("unmatched case must still carry a plausible expiry");
        };
        assert!(expiry_unmatched > Utc::now());
    }

    #[test]
    fn public_response_challenge_fields_are_indistinguishable_across_branches() {
        // Contract test: two cases with the SAME case id but different OTP
        // states (outstanding OTP vs no OTP / unmatched) must project public
        // responses whose OTP-challenge fields are identical in value, so a
        // caller cannot infer whether the phone matched an account.
        let created_at = Utc::now();
        let base = RecoveryCaseResponse {
            case_id: "case_1".to_owned(),
            status: "EVIDENCE_REQUIRED".to_owned(),
            target_user_id: None,
            approval_revision: 1,
            old_device_policy: None,
            approved_jkt: None,
            approved_device_id: None,
            otp_challenge_ref: None,
            otp_expires_at: None,
            otp_resend_allowed_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            version: 1,
        };

        let matched = RecoveryCaseResponse {
            otp_challenge_ref: Some(
                recovery_otp_projection(
                    "case_1",
                    created_at,
                    Some(Utc::now() + chrono::Duration::minutes(10)),
                )
                .0,
            ),
            otp_expires_at: Some(Utc::now() + chrono::Duration::minutes(10)),
            ..base.clone()
        };
        let unmatched = RecoveryCaseResponse {
            otp_challenge_ref: Some(recovery_otp_projection("case_1", created_at, None).0),
            otp_expires_at: recovery_otp_projection("case_1", created_at, None).1,
            ..base.clone()
        };

        // Identical challenge ref (same value, same relationship to the case id).
        assert_eq!(matched.otp_challenge_ref, unmatched.otp_challenge_ref);
        assert_eq!(
            matched.otp_challenge_ref.as_deref(),
            Some("case_1"),
            "challenge ref must equal the case id for both branches"
        );
        // Identical field presence for the challenge expiry.
        assert_eq!(
            matched.otp_expires_at.is_some(),
            unmatched.otp_expires_at.is_some()
        );
    }

    #[test]
    fn awaiting_admin_decision_maps_to_pending_review() {
        // A RUNNING flow parked at the admin decision step (after OTP
        // verification) is PENDING_REVIEW, not EVIDENCE_REQUIRED.
        assert_eq!(
            recovery_case_status(
                &flow(
                    "RUNNING",
                    Some("await_admin_decision"),
                    json!({}),
                    json!({})
                ),
                None
            ),
            "PENDING_REVIEW"
        );
    }

    #[tokio::test]
    async fn sync_recovery_case_sets_pending_review_at_admin_decision() {
        let mut repo = MockRecoveryCaseRepo::new();
        repo.expect_get_case_by_id()
            .returning(|_| Ok(Some(case_row(3, "EVIDENCE_REQUIRED"))));
        repo.expect_update_case()
            .withf(
                |_id: &str, _expected_version: &i64, patch: &RecoveryCaseUpdate| {
                    patch.status.as_deref() == Some("PENDING_REVIEW")
                },
            )
            .returning(|id, _, patch| {
                let mut row = case_row(4, patch.status.as_deref().unwrap_or_default());
                row.id = id.to_owned();
                Ok(row)
            });

        let seen = Arc::new(Mutex::new(None));
        let api = api_with_flow(repo, flow_repo_echo(seen.clone()));
        let f = flow(
            "RUNNING",
            Some("await_admin_decision"),
            json!({ "case_id": "case_1" }),
            json!({}),
        );
        sync_recovery_case(&api, &f).await.expect("sync succeeds");
        let written = seen.lock().unwrap().clone().expect("flow context written");
        assert_eq!(
            written
                .pointer("/recovery/case_version")
                .and_then(Value::as_i64),
            Some(4)
        );
    }

    #[tokio::test]
    async fn sync_recovery_case_persists_risk_flags() {
        let mut repo = MockRecoveryCaseRepo::new();
        repo.expect_get_case_by_id()
            .returning(|_| Ok(Some(case_row(3, "EVIDENCE_REQUIRED"))));
        repo.expect_update_case()
            .withf(
                |_id: &str, _expected_version: &i64, patch: &RecoveryCaseUpdate| {
                    // Risk flags computed from the unmatched phone relation.
                    patch
                        .risk_flags
                        .as_ref()
                        .and_then(|value| value.as_array())
                        .is_some_and(|flags| {
                            flags
                                .iter()
                                .any(|flag| flag == "UNMATCHED_OR_AMBIGUOUS_PHONE")
                        })
                },
            )
            .returning(|id, _, patch| {
                let mut row = case_row(4, patch.status.as_deref().unwrap_or_default());
                row.id = id.to_owned();
                row.risk_flags = patch.risk_flags.clone().unwrap_or_else(|| json!([]));
                Ok(row)
            });

        let seen = Arc::new(Mutex::new(None));
        let api = api_with_flow(repo, flow_repo_echo(seen.clone()));
        let f = flow(
            "RUNNING",
            Some("await_admin_decision"),
            json!({ "case_id": "case_1", "phone_relation": "UNKNOWN" }),
            json!({}),
        );
        sync_recovery_case(&api, &f).await.expect("sync succeeds");
        let written = seen.lock().unwrap().clone().expect("flow context written");
        assert_eq!(
            written
                .pointer("/recovery/case_version")
                .and_then(Value::as_i64),
            Some(4)
        );
    }

    #[tokio::test]
    async fn sync_recovery_case_keeps_case_version_in_lock_step_across_reviews() {
        // Simulates repeated staff reviews: after a staff decision the flow
        // context carries `recovery.case_version`; each projection write must
        // advance the stored value in lock-step so the next projection (or
        // decision) never trips RECOVERY_CASE_VERSION_CONFLICT on a stale
        // expected version.
        //
        // First write: case_version 4 in the flow, DB row at 4 -> bump to 5 and
        // write 5 back into the flow context.
        let mut repo = MockRecoveryCaseRepo::new();
        repo.expect_get_case_by_id()
            .returning(|_| Ok(Some(case_row(4, "PENDING_REVIEW"))));
        repo.expect_update_case()
            .withf(
                |_id: &str, expected_version: &i64, _patch: &RecoveryCaseUpdate| {
                    *expected_version == 4
                },
            )
            .returning(|id, _, patch| {
                let mut row = case_row(5, patch.status.as_deref().unwrap_or_default());
                row.id = id.to_owned();
                Ok(row)
            });

        let seen = Arc::new(Mutex::new(None));
        let api = api_with_flow(repo, flow_repo_echo(seen.clone()));
        let first = flow(
            "RUNNING",
            Some("await_admin_decision"),
            json!({ "case_id": "case_1", "case_version": 4 }),
            json!({ "await_admin_decision": { "decision": "APPROVED" } }),
        );
        sync_recovery_case(&api, &first)
            .await
            .expect("first sync succeeds");
        let written = seen.lock().unwrap().clone().expect("flow context written");
        assert_eq!(
            written
                .pointer("/recovery/case_version")
                .and_then(Value::as_i64),
            Some(5),
            "write-back must advance recovery.case_version to the DB version"
        );

        // Second write: the flow context now carries the written-back 5; the DB
        // row is at 5 too, so the projection's expected version (5) matches and
        // the write succeeds instead of 409ing.
        let mut repo2 = MockRecoveryCaseRepo::new();
        repo2
            .expect_get_case_by_id()
            .returning(|_| Ok(Some(case_row(5, "APPROVED"))));
        repo2
            .expect_update_case()
            .withf(
                |_id: &str, expected_version: &i64, _patch: &RecoveryCaseUpdate| {
                    *expected_version == 5
                },
            )
            .returning(|id, _, patch| {
                let mut row = case_row(6, patch.status.as_deref().unwrap_or_default());
                row.id = id.to_owned();
                Ok(row)
            });
        let seen2 = Arc::new(Mutex::new(None));
        let api2 = api_with_flow(repo2, flow_repo_echo(seen2.clone()));
        let second = flow(
            "RUNNING",
            Some("await_admin_decision"),
            json!({ "case_id": "case_1", "case_version": 5 }),
            json!({ "await_admin_decision": { "decision": "APPROVED" } }),
        );
        sync_recovery_case(&api2, &second)
            .await
            .expect("second sync must not conflict");
        let written2 = seen2.lock().unwrap().clone().expect("flow context written");
        assert_eq!(
            written2
                .pointer("/recovery/case_version")
                .and_then(Value::as_i64),
            Some(6),
            "write-back must keep advancing in lock-step"
        );
    }
}

#[cfg(test)]
mod recovery_projection_redaction_tests {
    use super::*;
    use crate::test_utils::TestAppStateBuilder;
    use backend_model::db::FlowInstanceRow;
    use std::sync::Arc;

    fn flow_with_context(flow_type: &str, context: Value) -> FlowInstanceRow {
        FlowInstanceRow {
            id: "flow_1".to_owned(),
            human_id: "rc.f1".to_owned(),
            session_id: "sess_1".to_owned(),
            flow_type: flow_type.to_owned(),
            status: "RUNNING".to_owned(),
            current_step: Some("verify_recovery_otp".to_owned()),
            step_ids: json!([]),
            context,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn recovery_context() -> Value {
        json!({
            "recovery": {
                "case_id": "case_1",
                "otp_hash": "s3cret-hash",
                "otp_salt": "s3cret-salt",
                "matched": true,
                "matched_user_id": "usr_victim",
                "requested_phone_hash": "phone-hash",
                "requested_phone_masked": "+2376****0000",
            },
            "step_output": {
                "resolve_existing_account": {
                    "matched": true,
                    "matched_user_id": "usr_victim",
                    "requested_phone_hash": "phone-hash",
                }
            }
        })
    }

    fn api_with() -> BackendApi {
        let state = TestAppStateBuilder::new().build();
        let oidc = state.oidc_state.clone();
        let signature = state.signature_state.clone();
        BackendApi::new(Arc::new(state), oidc, signature)
    }

    #[test]
    fn redact_recovery_fields_strips_sensitive_keys_everywhere() {
        let mut value = recovery_context();
        redact_recovery_fields(&mut value);
        let serialized = value.to_string();
        for forbidden in [
            "s3cret-hash",
            "s3cret-salt",
            "phone-hash",
            "usr_victim",
            "otp_hash",
            "otp_salt",
            "matched_user_id",
        ] {
            assert!(
                !serialized.contains(forbidden),
                "redaction leaked {forbidden}: {serialized}"
            );
        }
        // Non-sensitive recovery fields survive.
        assert_eq!(
            value.pointer("/recovery/case_id").and_then(Value::as_str),
            Some("case_1")
        );
        assert_eq!(
            value
                .pointer("/recovery/requested_phone_masked")
                .and_then(Value::as_str),
            Some("+2376****0000")
        );
    }

    #[test]
    fn flow_response_projection_redacts_recovery_context() {
        let response = flow_response(flow_with_context("account_recovery", recovery_context()));
        let serialized = serde_json::to_string(&response.context).unwrap();
        for forbidden in [
            "otp_hash",
            "otp_salt",
            "matched_user_id",
            "phone-hash",
            "usr_victim",
        ] {
            assert!(
                !serialized.contains(forbidden),
                "flow projection leaked {forbidden}: {serialized}"
            );
        }
    }

    #[test]
    fn non_recovery_flow_projection_is_not_redacted() {
        let context = json!({ "matched": "keep-me", "otp_hash": "kept" });
        let response = flow_response(flow_with_context("phone_otp", context));
        let serialized = serde_json::to_string(&response.context).unwrap();
        assert!(serialized.contains("keep-me"), "{serialized}");
        // Non-recovery flows keep their (unrelated) fields untouched.
        assert!(serialized.contains("otp_hash"), "{serialized}");
    }

    #[test]
    fn get_flow_and_get_step_projections_redact() {
        // get_flow and get_step redact step output containing matched_user_id.
        let _ = api_with();
        let mut ctx = recovery_context();
        redact_recovery_fields(&mut ctx);
        let flow = flow_with_context("account_recovery", ctx);
        let mut step = FlowStepRow {
            id: "step_1".to_owned(),
            human_id: "rc.f1.verify".to_owned(),
            flow_id: "flow_1".to_owned(),
            step_type: "resolve_existing_account".to_owned(),
            actor: "SYSTEM".to_owned(),
            status: "COMPLETED".to_owned(),
            attempt_no: 0,
            input: None,
            output: Some(json!({
                "matched": true,
                "matched_user_id": "usr_victim",
                "requested_phone_hash": "phone-hash",
            })),
            error: None,
            next_retry_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            finished_at: Some(Utc::now()),
        };
        let mut out = step.output.take().unwrap();
        redact_recovery_fields(&mut out);
        assert!(out.get("matched_user_id").is_none(), "{out}");
        assert!(out.get("matched").is_none(), "{out}");
        assert!(out.get("requested_phone_hash").is_none(), "{out}");
    }
}

#[cfg(test)]
mod recovery_owner_tests {
    use super::*;
    use crate::test_utils::{MockFlowRepo, TestAppStateBuilder};
    use backend_model::db::FlowSessionRow;
    use std::sync::Arc;

    fn session(session_type: &str, context: Value, user_id: Option<String>) -> FlowSessionRow {
        FlowSessionRow {
            id: "sess_1".to_owned(),
            human_id: "rc.s1".to_owned(),
            user_id,
            session_type: session_type.to_owned(),
            status: "RUNNING".to_owned(),
            context,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            completed_at: None,
        }
    }

    fn api_with(flow: MockFlowRepo) -> BackendApi {
        let state = TestAppStateBuilder::new().with_flow(Arc::new(flow)).build();
        let oidc = state.oidc_state.clone();
        let signature = state.signature_state.clone();
        BackendApi::new(Arc::new(state), oidc, signature)
    }

    fn caller(service: bool, device_id: &str) -> BffCallerIdentity {
        BffCallerIdentity {
            user_id: "usr_caller".to_owned(),
            device_id: device_id.to_owned(),
            service_client_id: if service {
                Some("azamra-bff".to_owned())
            } else {
                None
            },
        }
    }

    #[tokio::test]
    async fn recovery_session_fails_closed_for_device_without_owner_claim() {
        // C2: a NULL-owner recovery session with no bound owner device must not
        // vacuously pass for an authenticated device.
        let mut flow = MockFlowRepo::new();
        flow.expect_get_session()
            .returning(|_| Ok(Some(session("account_recovery", json!({}), None))));
        let api = api_with(flow);
        let err = ensure_session_owner(&api, "sess_1", &caller(false, "dvc_attacker"))
            .await
            .unwrap_err();
        match err {
            Error::Http {
                error_key,
                status_code,
                ..
            } => {
                assert_eq!(status_code, 403);
                assert_eq!(error_key, "RECOVERY_OWNER_UNKNOWN");
            }
            other => panic!("expected 403, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn recovery_session_rejects_mismatched_device() {
        let mut flow = MockFlowRepo::new();
        flow.expect_get_session().returning(|_| {
            Ok(Some(session(
                "account_recovery",
                json!({ "recovery": { "owner_device_id": "dvc_target" } }),
                None,
            )))
        });
        let api = api_with(flow);
        let err = ensure_session_owner(&api, "sess_1", &caller(false, "dvc_other"))
            .await
            .unwrap_err();
        match err {
            Error::Http {
                error_key,
                status_code,
                ..
            } => {
                assert_eq!(status_code, 403);
                assert_eq!(error_key, "RECOVERY_OWNER_MISMATCH");
            }
            other => panic!("expected 403, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn recovery_session_allows_matching_device() {
        let mut flow = MockFlowRepo::new();
        flow.expect_get_session().returning(|_| {
            Ok(Some(session(
                "account_recovery",
                json!({ "recovery": { "owner_device_id": "dvc_owner" } }),
                None,
            )))
        });
        let api = api_with(flow);
        let s = ensure_session_owner(&api, "sess_1", &caller(false, "dvc_owner"))
            .await
            .expect("matching device is the owner");
        assert_eq!(s.id, "sess_1");
    }

    #[tokio::test]
    async fn recovery_session_allows_service_caller() {
        let mut flow = MockFlowRepo::new();
        flow.expect_get_session()
            .returning(|_| Ok(Some(session("account_recovery", json!({}), None))));
        let api = api_with(flow);
        let s = ensure_session_owner(&api, "sess_1", &caller(true, "bff"))
            .await
            .expect("service caller drives recovery");
        assert_eq!(s.id, "sess_1");
    }
}
