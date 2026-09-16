use super::BackendApi;
use super::bff_flow::{
    FlowDetailResponse, FlowResponse, SessionResponse, StepResponse, SubmitStepRequest,
    service as bff_service,
};
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::{Json, Router, routing::get};
use backend_auth::JwtToken;
use backend_core::Error;
use backend_flow_sdk::{StepContext, StepOutcome};
use backend_repository::{
    FlowSessionFilter, FlowStepPatch, RecoveryCaseFilter, RecoveryCaseUpdate,
};
use chrono::DateTime;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use tracing::instrument;
use utoipa::{OpenApi, ToSchema};

#[derive(OpenApi)]
#[openapi(
    paths(
        list_staff_sessions,
        get_staff_session,
        get_staff_flow,
        list_admin_steps,
        get_admin_step,
        submit_admin_step,
        list_recovery_cases,
        list_recovery_case_events,
    ),
    components(schemas(
        StaffSessionQuery,
        StaffSessionResponse,
        StaffSessionDetailResponse,
        StaffSessionListResponse,
        AdminStepQuery,
        SubmitStepRequest,
        SessionResponse,
        FlowResponse,
        FlowDetailResponse,
        StepResponse
    )),
    tags((name = "staff-flow", description = "Staff flow v2 endpoints"))
)]
pub struct StaffFlowOpenApi;

pub fn router(api: BackendApi) -> Router {
    Router::new()
        .route("/sessions", get(list_staff_sessions))
        .route("/sessions/{session_id}", get(get_staff_session))
        .route("/flows/{flow_id}", get(get_staff_flow))
        .route("/steps", get(list_admin_steps))
        .route(
            "/steps/{step_id}",
            get(get_admin_step).post(submit_admin_step),
        )
        .route("/recovery-cases", get(list_recovery_cases))
        .route(
            "/recovery-cases/{case_id}/events",
            get(list_recovery_case_events),
        )
        .with_state(api)
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryCaseQuery {
    pub status: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StaffRecoveryCaseResponse {
    pub case_id: String,
    pub status: String,
    pub target_user_id: Option<String>,
    pub approval_revision: i64,
    pub old_device_policy: Option<String>,
    pub approved_jkt: Option<String>,
    pub approved_device_id: Option<String>,
    pub otp_challenge_ref: Option<String>,
    pub otp_expires_at: Option<DateTime<Utc>>,
    pub otp_resend_allowed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub version: i64,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StaffRecoveryEventResponse {
    pub id: String,
    pub case_id: String,
    pub event: String,
    pub actor: String,
    pub actor_type: String,
    pub created_at: DateTime<Utc>,
    pub details: serde_json::Value,
}

#[utoipa::path(get, path = "/flow/recovery-cases", responses((status = 200, body = [StaffRecoveryCaseResponse])), tag = "staff-flow")]
async fn list_recovery_cases(
    State(api): State<BackendApi>,
    headers: HeaderMap,
    Query(query): Query<RecoveryCaseQuery>,
) -> Result<Json<Vec<StaffRecoveryCaseResponse>>, Error> {
    let _token = require_staff_token(&api, &headers).await?;
    let (rows, _) = api
        .state
        .recovery_case
        .list_cases(RecoveryCaseFilter {
            status: query.status,
            matched_user_id: None,
            page: 1,
            limit: 100,
        })
        .await?;
    Ok(Json(
        rows.into_iter()
            .map(|row| StaffRecoveryCaseResponse {
                case_id: row.id.clone(),
                status: row.status,
                target_user_id: row.matched_user_id,
                approval_revision: row.approval_revision.unwrap_or(1),
                old_device_policy: row
                    .old_devices
                    .get("policy")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned),
                approved_jkt: row.jkt,
                approved_device_id: row.device_id,
                otp_challenge_ref: row.otp_hash.as_ref().map(|_| row.id),
                otp_expires_at: row.otp_expires_at,
                otp_resend_allowed_at: row.otp_resend_at,
                created_at: row.created_at,
                updated_at: row.updated_at,
                version: row.version,
            })
            .collect(),
    ))
}

#[utoipa::path(get, path = "/flow/recovery-cases/{caseId}/events", responses((status = 200, body = [StaffRecoveryEventResponse])), tag = "staff-flow")]
async fn list_recovery_case_events(
    State(api): State<BackendApi>,
    headers: HeaderMap,
    Path(case_id): Path<String>,
) -> Result<Json<Vec<StaffRecoveryEventResponse>>, Error> {
    let _token = require_staff_token(&api, &headers).await?;
    let case = api
        .state
        .recovery_case
        .get_case_by_id(&case_id)
        .await?
        .ok_or_else(|| Error::not_found("RECOVERY_CASE_NOT_FOUND", "Recovery case not found"))?;
    let mut events = Vec::new();
    if let Some(session_id) = case.session_id {
        for flow in api.state.flow.list_flows_for_session(&session_id).await? {
            if !flow.flow_type.eq_ignore_ascii_case("account_recovery") {
                continue;
            }
            for step in api.state.flow.list_steps_for_flow(&flow.id).await? {
                events.push(StaffRecoveryEventResponse {
                    id: step.id,
                    case_id: case_id.clone(),
                    event: format!("{}_{}", step.step_type, step.status).to_uppercase(),
                    actor: step.actor.clone(),
                    actor_type: step.actor,
                    created_at: step.updated_at,
                    details: json!({ "attempt": step.attempt_no }),
                });
            }
        }
    }
    events.sort_by_key(|event| event.created_at);
    Ok(Json(events))
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StaffSessionQuery {
    #[serde(default)]
    pub user_id: Option<String>,
    #[serde(default)]
    pub phone_number: Option<String>,
    #[serde(default)]
    pub session_type: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default = "default_page")]
    pub page: i32,
    #[serde(default = "default_limit")]
    pub limit: i32,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AdminStepQuery {
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub user_id: Option<String>,
    #[serde(default)]
    pub phone_number: Option<String>,
    #[serde(default)]
    pub flow_type: Option<String>,
}

#[derive(Debug, serde::Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StaffSessionResponse {
    pub id: String,
    pub human_id: String,
    pub session_type: String,
    pub status: String,
    pub user_id: Option<String>,
    pub phone_number: Option<String>,
    pub full_name: Option<String>,
    pub context: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, serde::Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StaffSessionDetailResponse {
    pub session: StaffSessionResponse,
    pub flows: Vec<FlowResponse>,
}

#[derive(Debug, serde::Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StaffSessionListResponse {
    pub items: Vec<StaffSessionResponse>,
    pub page: i32,
    pub limit: i32,
    pub total: i64,
}

fn default_page() -> i32 {
    1
}

fn default_limit() -> i32 {
    50
}

#[utoipa::path(
    get,
    path = "/flow/sessions",
    params(
        ("userId" = Option<String>, Query),
        ("phoneNumber" = Option<String>, Query),
        ("sessionType" = Option<String>, Query),
        ("status" = Option<String>, Query),
        ("page" = Option<i32>, Query),
        ("limit" = Option<i32>, Query)
    ),
    responses((status = 200, body = StaffSessionListResponse)),
    tag = "staff-flow",
    security(("bearerAuth" = []))
)]
#[instrument(skip(api, headers))]
async fn list_staff_sessions(
    State(api): State<BackendApi>,
    headers: HeaderMap,
    Query(query): Query<StaffSessionQuery>,
) -> Result<Json<StaffSessionListResponse>, Error> {
    let _token = require_staff_token(&api, &headers).await?;

    let user_ids = resolve_user_ids_for_filters(
        &api,
        query.user_id.as_deref(),
        query.phone_number.as_deref(),
    )
    .await?;
    if query.phone_number.is_some() && user_ids.is_empty() {
        return Ok(Json(StaffSessionListResponse {
            items: Vec::new(),
            page: query.page.max(1),
            limit: query.limit.clamp(1, 100),
            total: 0,
        }));
    }

    let filter = FlowSessionFilter {
        user_id: query.user_id.clone(),
        user_ids: if query.phone_number.is_some() {
            Some(user_ids)
        } else {
            None
        },
        session_type: query.session_type.clone(),
        status: query.status.clone(),
        page: query.page,
        limit: query.limit,
    }
    .normalized();

    let (rows, total) = api.state.flow.list_sessions(filter.clone()).await?;
    let users = load_users_for_sessions(&api, &rows).await?;
    let items = rows
        .into_iter()
        .map(|row| build_staff_session_response(row, &users))
        .collect();

    Ok(Json(StaffSessionListResponse {
        items,
        page: filter.page,
        limit: filter.limit,
        total,
    }))
}

#[utoipa::path(
    get,
    path = "/flow/sessions/{session_id}",
    params(("session_id" = String, Path)),
    responses((status = 200, body = StaffSessionDetailResponse)),
    tag = "staff-flow",
    security(("bearerAuth" = []))
)]
#[instrument(skip(api, headers))]
async fn get_staff_session(
    State(api): State<BackendApi>,
    Path(session_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<StaffSessionDetailResponse>, Error> {
    let _token = require_staff_token(&api, &headers).await?;

    let session = api
        .state
        .flow
        .get_session(&session_id)
        .await?
        .ok_or_else(|| Error::not_found("SESSION_NOT_FOUND", "Session not found"))?;
    let flows = api.state.flow.list_flows_for_session(&session.id).await?;
    let users = load_users_for_sessions(&api, std::slice::from_ref(&session)).await?;

    Ok(Json(StaffSessionDetailResponse {
        session: build_staff_session_response(session, &users),
        flows: flows.into_iter().map(Into::into).collect(),
    }))
}

#[utoipa::path(
    get,
    path = "/flow/flows/{flow_id}",
    params(("flow_id" = String, Path)),
    responses((status = 200, body = FlowDetailResponse)),
    tag = "staff-flow",
    security(("bearerAuth" = []))
)]
#[instrument(skip(api, headers))]
async fn get_staff_flow(
    State(api): State<BackendApi>,
    Path(flow_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<FlowDetailResponse>, Error> {
    let _token = require_staff_token(&api, &headers).await?;
    let flow = api
        .state
        .flow
        .get_flow(&flow_id)
        .await?
        .ok_or_else(|| Error::not_found("FLOW_NOT_FOUND", "Flow not found"))?;
    let steps = api.state.flow.list_steps_for_flow(&flow_id).await?;

    Ok(Json(FlowDetailResponse {
        flow: flow.into(),
        steps: steps.into_iter().map(Into::into).collect(),
    }))
}

#[utoipa::path(
    get,
    path = "/flow/steps",
    params(
        ("status" = Option<String>, Query),
        ("userId" = Option<String>, Query),
        ("phoneNumber" = Option<String>, Query),
        ("flowType" = Option<String>, Query)
    ),
    responses((status = 200, body = [StepResponse])),
    tag = "staff-flow",
    security(("bearerAuth" = []))
)]
#[instrument(skip(api, headers))]
async fn list_admin_steps(
    State(api): State<BackendApi>,
    headers: HeaderMap,
    Query(query): Query<AdminStepQuery>,
) -> Result<Json<Vec<StepResponse>>, Error> {
    let _token = require_staff_token(&api, &headers).await?;

    let user_ids = resolve_user_ids_for_filters(
        &api,
        query.user_id.as_deref(),
        query.phone_number.as_deref(),
    )
    .await?;
    if query.phone_number.is_some() && user_ids.is_empty() {
        return Ok(Json(Vec::new()));
    }

    let (sessions, _) = api
        .state
        .flow
        .list_sessions(FlowSessionFilter {
            user_id: query.user_id.clone(),
            user_ids: if query.phone_number.is_some() {
                Some(user_ids)
            } else {
                None
            },
            session_type: None,
            status: None,
            page: 1,
            limit: 500,
        })
        .await?;

    let mut steps: Vec<StepResponse> = Vec::new();
    for session in sessions {
        let flows = api.state.flow.list_flows_for_session(&session.id).await?;
        for flow in flows {
            if let Some(flow_type) = query.flow_type.as_deref()
                && !flow.flow_type.eq_ignore_ascii_case(flow_type)
            {
                continue;
            }

            let flow_steps = api.state.flow.list_steps_for_flow(&flow.id).await?;
            for step in flow_steps {
                if !step.actor.eq_ignore_ascii_case("ADMIN") {
                    continue;
                }
                if let Some(status) = query.status.as_deref()
                    && !step.status.eq_ignore_ascii_case(status)
                {
                    continue;
                }
                steps.push(step.into());
            }
        }
    }

    steps.sort_by(|left, right| {
        left.created_at
            .cmp(&right.created_at)
            .then_with(|| left.id.cmp(&right.id))
    });

    Ok(Json(steps))
}

#[utoipa::path(
    get,
    path = "/flow/steps/{step_id}",
    params(("step_id" = String, Path)),
    responses((status = 200, body = StepResponse)),
    tag = "staff-flow",
    security(("bearerAuth" = []))
)]
#[instrument(skip(api, headers))]
async fn get_admin_step(
    State(api): State<BackendApi>,
    Path(step_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<StepResponse>, Error> {
    let _token = require_staff_token(&api, &headers).await?;
    let step = get_admin_step_row(&api, &step_id).await?;
    Ok(Json(step.into()))
}

#[utoipa::path(
    post,
    path = "/flow/steps/{step_id}",
    params(("step_id" = String, Path)),
    request_body = SubmitStepRequest,
    responses((status = 200, body = StepResponse)),
    tag = "staff-flow",
    security(("bearerAuth" = []))
)]
#[instrument(skip(api, headers))]
async fn submit_admin_step(
    State(api): State<BackendApi>,
    Path(step_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<SubmitStepRequest>,
) -> Result<Json<StepResponse>, Error> {
    let _token = require_staff_token(&api, &headers).await?;

    let step = get_admin_step_row(&api, &step_id).await?;
    if !step.status.eq_ignore_ascii_case("WAITING") {
        return Err(Error::conflict(
            "STEP_NOT_WAITING",
            "Admin step is not waiting for input",
        ));
    }
    let flow = api
        .state
        .flow
        .get_flow(&step.flow_id)
        .await?
        .ok_or_else(|| Error::not_found("FLOW_NOT_FOUND", "Flow not found"))?;
    let session = api
        .state
        .flow
        .get_session(&flow.session_id)
        .await?
        .ok_or_else(|| Error::not_found("SESSION_NOT_FOUND", "Session not found"))?;

    // Recovery admin decisions bypass the generic WAIT validation: they must
    // validate the reviewer's expectedVersion and enforce the recovery
    // checklist. We only VALIDATE here (no case mutation yet): the actual
    // version-checked transition on the case is deferred until after the flow
    // transition below is committed, so a failed flow step can never leave the
    // case APPROVED while the flow never advances.
    let recovery_decision = validate_recovery_decision(&api, &flow, &body).await?;

    let flow_definition = bff_service::get_flow_definition(&api, &flow.flow_type)?;
    let step_definition = bff_service::get_step_definition(flow_definition, &step.step_type)?;

    step_definition
        .validate_input(&body.input)
        .await
        .map_err(bff_service::flow_error_to_http)?;

    let verify_context = StepContext {
        session_id: session.id.clone(),
        session_user_id: session.user_id.clone(),
        flow_id: flow.id.clone(),
        step_id: step.id.clone(),
        input: body.input.clone(),
        session_context: session.context.clone(),
        flow_context: flow.context.clone(),
        services: crate::flows::runtime::step_services_with_device(
            api.state.user.clone(),
            api.state.device.clone(),
        ),
    };

    let verify_outcome = step_definition
        .verify_input(&verify_context, &body.input)
        .await
        .map_err(bff_service::flow_error_to_http)?;

    let (output_value, context_updates, branch, status) = match verify_outcome {
        StepOutcome::Done { output, updates } => (
            output.unwrap_or_else(|| json!({"verified": true})),
            updates,
            None,
            "COMPLETED",
        ),
        StepOutcome::Branched {
            branch,
            output,
            updates,
        } => (
            output.unwrap_or_else(|| json!({"verified": true})),
            updates,
            Some(branch),
            "COMPLETED",
        ),
        StepOutcome::Failed { error, retryable } => {
            let session_id = flow.session_id.clone();
            let updated = api
                .state
                .flow
                .patch_step(
                    &step_id,
                    FlowStepPatch::new()
                        .status("FAILED")
                        .input(body.input.clone())
                        .error(json!({"error": error, "retryable": retryable}))
                        .finished_at(Utc::now()),
                )
                .await?;

            if let Some(next_step) = crate::flows::runtime::resolve_transition(
                flow_definition,
                &step.step_type,
                None,
                true,
            ) {
                if bff_service::has_flow_step(flow_definition, &next_step) {
                    bff_service::create_step_chain(&api, &session, flow, next_step, None).await?;
                } else {
                    bff_service::finalize_flow(
                        &api,
                        &flow,
                        bff_service::terminal_status(&next_step),
                    )
                    .await?;
                }
                bff_service::refresh_session_status(&api, &session_id).await?;
            } else {
                bff_service::finalize_flow(&api, &flow, "FAILED").await?;
            }

            return Ok(Json(updated.into()));
        }
        StepOutcome::Waiting { .. } | StepOutcome::Retry { .. } => {
            return Err(Error::conflict(
                "INVALID_ADMIN_STEP_OUTCOME",
                "Admin submission must resolve to a terminal verification outcome",
            ));
        }
    };

    let mut updated_flow_context =
        bff_service::store_step_output(flow.context.clone(), &step.step_type, &body.input);
    if let Some(updates) = context_updates {
        if let Some(flow_patch) = updates.flow_context_patch.as_ref() {
            updated_flow_context =
                crate::flows::runtime::merged_json(updated_flow_context, flow_patch);
        }
        bff_service::apply_context_updates(&api, &session, updates).await?;
    }
    if recovery_decision.is_some() {
        updated_flow_context = crate::flows::runtime::merged_json(
            updated_flow_context,
            &json!({ "recovery": { "decision_pending": true } }),
        );
    }

    let updated_step = api
        .state
        .flow
        .patch_step(
            &step_id,
            FlowStepPatch::new()
                .status(status)
                .input(body.input.clone())
                .output(output_value)
                .clear_error()
                .finished_at(Utc::now()),
        )
        .await?;

    let mut current_flow = api
        .state
        .flow
        .update_flow(&flow.id, None, None, None, Some(updated_flow_context))
        .await?;

    if let Some(next_step) = crate::flows::runtime::resolve_transition(
        flow_definition,
        &updated_step.step_type,
        branch.as_deref(),
        false,
    ) {
        if bff_service::has_flow_step(flow_definition, &next_step) {
            current_flow =
                bff_service::create_step_chain(&api, &session, current_flow, next_step, None)
                    .await?;
        } else {
            current_flow = bff_service::finalize_flow(
                &api,
                &current_flow,
                bff_service::terminal_status(&next_step),
            )
            .await?;
        }
    } else {
        current_flow = bff_service::finalize_flow(&api, &current_flow, "COMPLETED").await?;
    }

    // Apply the recovery decision only now, after the flow transition has been
    // committed. This guarantees the case can never be left APPROVED while the
    // flow step/transition did not advance. The version-checked `update_case`
    // (using the reviewer's expectedVersion) still rejects concurrent/stale
    // reviewers with a 409.
    if let Some(decision) = recovery_decision {
        let recovery_context_patch = apply_recovery_decision_update(&api, &decision).await?;
        if let Some(recovery_context_patch) = recovery_context_patch {
            let mut context = current_flow.context.clone();
            context = crate::flows::runtime::merged_json(
                context,
                &crate::flows::runtime::merged_json(
                    recovery_context_patch,
                    &json!({ "recovery": { "decision_pending": false } }),
                ),
            );
            current_flow = api
                .state
                .flow
                .update_flow(&current_flow.id, None, None, None, Some(context))
                .await?;
            bff_service::sync_recovery_case(&api, &current_flow).await?;
        }
    }

    bff_service::refresh_session_status(&api, &current_flow.session_id).await?;
    Ok(Json(updated_step.into()))
}

/// A validated recovery decision that has not yet been applied to the case.
///
/// Validation (stale-version rejection, decision and checklist enforcement,
/// patch construction) happens up-front, while the actual atomic,
/// version-checked `update_case` is deferred until after the flow transition
/// has been committed (see `submit_admin_step`), so the case can never be left
/// APPROVED while the flow has not advanced.
struct RecoveryDecision {
    case_id: String,
    expected_version: i64,
    patch: RecoveryCaseUpdate,
}

/// Handles an account-recovery KYC Manager decision on `await_admin_decision`.
///
/// This intentionally does NOT use the generic WAIT-step validation. Instead it:
/// 1. validates the reviewer's submitted `expectedVersion` against the case's
///    current version (a stale reviewer is rejected with 409),
/// 2. enforces the recovery checklist,
/// 3. builds the case patch (applied later, after the flow transition commits,
///    through an atomic, version-checked `update_case`).
///
/// This is the validate-only half: it performs no mutation of the case. The
/// caller applies the returned decision via `apply_recovery_decision_update`
/// only once the flow transition has been committed. Returns `None` when the
/// flow is not an account-recovery decision step.
async fn validate_recovery_decision(
    api: &BackendApi,
    flow: &backend_model::db::FlowInstanceRow,
    body: &SubmitStepRequest,
) -> Result<Option<RecoveryDecision>, Error> {
    if !flow.flow_type.eq_ignore_ascii_case("account_recovery") {
        return Ok(None);
    }

    let case_id = flow
        .context
        .pointer("/recovery/case_id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            Error::bad_request(
                "RECOVERY_CASE_NOT_FOUND",
                "Recovery flow has no recovery case",
            )
        })?;

    let case = api
        .state
        .recovery_case
        .get_case_by_id(case_id)
        .await?
        .ok_or_else(|| Error::not_found("RECOVERY_CASE_NOT_FOUND", "Recovery case not found"))?;

    let decision = body
        .input
        .get("decision")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_uppercase();

    if !matches!(
        decision.as_str(),
        "APPROVED" | "REJECTED" | "NEEDS_MORE_EVIDENCE"
    ) {
        return Err(Error::bad_request(
            "INVALID_RECOVERY_DECISION",
            "decision must be APPROVED, REJECTED or NEEDS_MORE_EVIDENCE",
        ));
    }

    // Reject stale reviewers before touching the case. The atomic update in
    // `apply_recovery_decision_update` re-checks the version, so a race between
    // two reviewers still yields 409. For APPROVED decisions, expectedVersion
    // is required (staff review). For REJECTED/NEEDS_MORE_EVIDENCE, it is
    // optional (user-initiated cancel may not provide it; the version-checked
    // `update_case` still protects via the case row version).
    let expected_version = if decision == "APPROVED" {
        body.input
            .get("expectedVersion")
            .and_then(serde_json::Value::as_i64)
            .ok_or_else(|| {
                Error::bad_request(
                    "MISSING_EXPECTED_VERSION",
                    "expectedVersion is required for approval",
                )
            })?
    } else {
        body.input
            .get("expectedVersion")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(case.version)
    };
    if expected_version != case.version {
        return Err(Error::conflict(
            "STALE_RECOVERY_VERSION",
            format!(
                "Recovery case {} was modified by another reviewer (expected version {expected_version}, current {})",
                case.id, case.version
            ),
        ));
    }

    // Checklist is required for APPROVED decisions (staff KYC review).
    // For REJECTED/NEEDS_MORE_EVIDENCE (user-initiated cancel), it is optional.
    let empty_checklist = serde_json::Value::Object(serde_json::Map::new());
    let checklist = if decision == "APPROVED" {
        body.input
            .get("checklist")
            .filter(|value| value.is_object())
            .ok_or_else(|| {
                Error::bad_request(
                    "RECOVERY_CHECKLIST_INCOMPLETE",
                    "checklist must be an object for approval",
                )
            })?
    } else {
        body.input
            .get("checklist")
            .filter(|value| value.is_object())
            .unwrap_or(&empty_checklist)
    };

    // For APPROVED decisions: the reviewer must confirm that identity was
    // verified and that the new device key proof was verified.
    // For REJECTED/NEEDS_MORE_EVIDENCE: no checklist enforcement.
    if decision == "APPROVED" {
        let identity_verified = checklist
            .get("kycIdentityVerified")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        if !identity_verified {
            return Err(Error::bad_request(
                "RECOVERY_CHECKLIST_INCOMPLETE",
                "checklist.kycIdentityVerified must be true",
            ));
        }
        let key_verified = checklist
            .get("deviceKeyProofVerified")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        if !key_verified {
            return Err(Error::bad_request(
                "RECOVERY_CHECKLIST_INCOMPLETE",
                "checklist.deviceKeyProofVerified must be true for approval",
            ));
        }
    }

    let status = match decision.as_str() {
        "APPROVED" => "APPROVED",
        "REJECTED" => "CLOSED",
        _ => "NEEDS_MORE_EVIDENCE",
    };

    let mut patch = RecoveryCaseUpdate {
        review_decision: Some(Some(decision.clone())),
        review_reason: Some(Some(
            body.input
                .get("reason")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .to_owned(),
        )),
        review_checklist: Some(Some(checklist.clone())),
        review_expected_version: Some(Some(expected_version)),
        status: Some(status.to_owned()),
        ..Default::default()
    };

    if decision == "APPROVED" {
        patch.approval_revision = Some(Some(case.approval_revision.unwrap_or(1).max(1) + 1));
        patch.approved_expires_at = Some(Some(Utc::now() + chrono::Duration::days(7)));
    }

    Ok(Some(RecoveryDecision {
        case_id: case_id.to_owned(),
        expected_version,
        patch,
    }))
}

/// Applies a validated recovery decision through an atomic, version-checked
/// transition on the case. Returns the flow-context patch carrying the
/// authoritative `recovery.case_version` produced by the update (consumed by
/// the `sync_recovery_case` projection).
async fn apply_recovery_decision_update(
    api: &BackendApi,
    decision: &RecoveryDecision,
) -> Result<Option<serde_json::Value>, Error> {
    let updated = api
        .state
        .recovery_case
        .update_case(
            &decision.case_id,
            decision.expected_version,
            &decision.patch,
        )
        .await?;
    Ok(Some(
        json!({ "recovery": { "case_version": updated.version } }),
    ))
}

/// Validates and immediately applies a recovery decision.
///
/// Kept as a convenience wrapper (validate + apply in one call) so callers that
/// do not need the deferred-apply ordering can use a single step.
async fn apply_recovery_decision(
    api: &BackendApi,
    flow: &backend_model::db::FlowInstanceRow,
    body: &SubmitStepRequest,
) -> Result<Option<serde_json::Value>, Error> {
    let Some(decision) = validate_recovery_decision(api, flow, body).await? else {
        return Ok(None);
    };
    apply_recovery_decision_update(api, &decision).await
}

async fn get_admin_step_row(
    api: &BackendApi,
    step_id: &str,
) -> Result<backend_model::db::FlowStepRow, Error> {
    let step = api
        .state
        .flow
        .get_step(step_id)
        .await?
        .ok_or_else(|| Error::not_found("STEP_NOT_FOUND", "Step not found"))?;

    if !step.actor.eq_ignore_ascii_case("ADMIN") {
        return Err(Error::bad_request(
            "STEP_NOT_ADMIN",
            "Step is not an admin-managed step",
        ));
    }

    Ok(step)
}

async fn require_staff_token(api: &BackendApi, headers: &HeaderMap) -> Result<JwtToken, Error> {
    if !api.state.config.staff.enabled {
        return Ok(JwtToken::new(backend_auth::Claims {
            sub: "usr_auth_disabled".to_owned(),
            azp: None,
            aud: None,
            scope: None,
            name: Some("auth-disabled".to_owned()),
            iss: api.state.config.oauth2.issuer.clone(),
            exp: usize::MAX,
            preferred_username: Some("auth-disabled".to_owned()),
        }));
    }

    let auth_header = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();

    if !auth_header.to_ascii_lowercase().starts_with("bearer ") {
        return Err(Error::unauthorized("Missing bearer token"));
    }

    JwtToken::verify(&auth_header[7..], &api.oidc_state).await
}

async fn resolve_user_ids_for_filters(
    api: &BackendApi,
    user_id: Option<&str>,
    phone_number: Option<&str>,
) -> Result<Vec<String>, Error> {
    let requested_user_id = user_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    let requested_phone = phone_number
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);

    let Some(phone_number) = requested_phone else {
        return Ok(requested_user_id.into_iter().collect());
    };

    let mut user_ids: Vec<String> = api
        .state
        .user
        .find_users_by_phone(None, &phone_number)
        .await?
        .into_iter()
        .map(|user| user.user_id)
        .collect();
    user_ids.sort();
    user_ids.dedup();

    if let Some(user_id) = requested_user_id {
        user_ids.retain(|candidate| candidate == &user_id);
    }

    Ok(user_ids)
}

async fn load_users_for_sessions(
    api: &BackendApi,
    sessions: &[backend_model::db::FlowSessionRow],
) -> Result<HashMap<String, backend_model::db::UserRow>, Error> {
    let mut users = HashMap::new();

    for user_id in sessions
        .iter()
        .filter_map(|session| session.user_id.clone())
    {
        if users.contains_key(&user_id) {
            continue;
        }
        if let Some(user) = api.state.user.get_user(&user_id).await? {
            users.insert(user_id, user);
        }
    }

    Ok(users)
}

fn build_staff_session_response(
    row: backend_model::db::FlowSessionRow,
    users: &HashMap<String, backend_model::db::UserRow>,
) -> StaffSessionResponse {
    let user_id = row.user_id.clone();
    let user = user_id.as_ref().and_then(|candidate| users.get(candidate));
    let phone_number = user.and_then(|value| value.phone_number.clone());
    let full_name = user.and_then(|value| value.full_name.clone());

    StaffSessionResponse {
        id: row.id,
        human_id: row.human_id,
        session_type: row.session_type,
        status: row.status,
        user_id,
        phone_number,
        full_name,
        context: row.context,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

#[cfg(test)]
mod recovery_decision_tests {
    use super::*;
    use crate::test_utils::{MockRecoveryCaseRepo, TestAppStateBuilder};
    use backend_model::db::{FlowInstanceRow, RecoveryCaseRow};
    use std::sync::Arc;

    fn flow_row() -> FlowInstanceRow {
        FlowInstanceRow {
            id: "flow_1".to_owned(),
            human_id: "rc.f1".to_owned(),
            session_id: "sess_1".to_owned(),
            flow_type: "account_recovery".to_owned(),
            status: "RUNNING".to_owned(),
            current_step: Some("await_admin_decision".to_owned()),
            step_ids: json!([]),
            context: json!({ "recovery": { "case_id": "case_1" } }),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn case_row(version: i64) -> RecoveryCaseRow {
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
            status: "PENDING_REVIEW".to_owned(),
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
            approval_revision: Some(2),
            evidence: json!({}),
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

    fn valid_input(decision: &str) -> serde_json::Value {
        json!({
            "decision": decision,
            "expectedVersion": 3,
            "reason": "verified",
            "checklist": {
                "kycIdentityVerified": true,
                "deviceKeyProofVerified": true,
            }
        })
    }

    fn stale_case_repo() -> MockRecoveryCaseRepo {
        let mut repo = MockRecoveryCaseRepo::new();
        repo.expect_get_case_by_id()
            .returning(|_| Ok(Some(case_row(3))));
        repo
    }

    #[tokio::test]
    async fn stale_reviewer_version_is_rejected_with_409() {
        let mut repo = MockRecoveryCaseRepo::new();
        // Case has already moved to version 5; reviewer still holds version 3.
        repo.expect_get_case_by_id()
            .returning(|_| Ok(Some(case_row(5))));
        let api = api_with(repo);
        let body = SubmitStepRequest {
            input: valid_input("APPROVED"),
        };
        let err = apply_recovery_decision(&api, &flow_row(), &body)
            .await
            .unwrap_err();
        match err {
            Error::Http {
                status_code,
                error_key,
                ..
            } => {
                assert_eq!(status_code, 409);
                assert_eq!(error_key, "STALE_RECOVERY_VERSION");
            }
            other => panic!("expected 409 Http error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn invalid_decision_is_rejected() {
        let api = api_with(stale_case_repo());
        let body = SubmitStepRequest {
            input: json!({ "decision": "MAYBE", "expectedVersion": 3, "checklist": {} }),
        };
        let err = apply_recovery_decision(&api, &flow_row(), &body)
            .await
            .unwrap_err();
        match err {
            Error::Http {
                status_code,
                error_key,
                ..
            } => {
                assert_eq!(status_code, 400);
                assert_eq!(error_key, "INVALID_RECOVERY_DECISION");
            }
            other => panic!("expected 400 Http error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn missing_checklist_is_rejected() {
        let api = api_with(stale_case_repo());
        let body = SubmitStepRequest {
            input: json!({ "decision": "APPROVED", "expectedVersion": 3 }),
        };
        let err = apply_recovery_decision(&api, &flow_row(), &body)
            .await
            .unwrap_err();
        match err {
            Error::Http {
                status_code,
                error_key,
                ..
            } => {
                assert_eq!(status_code, 400);
                assert_eq!(error_key, "RECOVERY_CHECKLIST_INCOMPLETE");
            }
            other => panic!("expected 400 Http error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn approval_requires_device_key_proof_verified() {
        let api = api_with(stale_case_repo());
        let body = SubmitStepRequest {
            input: json!({
                "decision": "APPROVED",
                "expectedVersion": 3,
                "checklist": { "kycIdentityVerified": true, "deviceKeyProofVerified": false }
            }),
        };
        let err = apply_recovery_decision(&api, &flow_row(), &body)
            .await
            .unwrap_err();
        match err {
            Error::Http {
                status_code,
                error_key,
                ..
            } => {
                assert_eq!(status_code, 400);
                assert_eq!(error_key, "RECOVERY_CHECKLIST_INCOMPLETE");
            }
            other => panic!("expected 400 Http error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn approval_uses_atomic_version_checked_transition_and_bumps_revision() {
        let mut repo = MockRecoveryCaseRepo::new();
        repo.expect_get_case_by_id()
            .returning(|_| Ok(Some(case_row(3))));
        repo.expect_update_case()
            .withf(
                |id: &str, expected_version: &i64, patch: &RecoveryCaseUpdate| {
                    id == "case_1"
                        && *expected_version == 3
                        && patch.review_decision.as_ref() == Some(&Some("APPROVED".to_owned()))
                        && patch.status.as_deref() == Some("APPROVED")
                        && patch.approval_revision == Some(Some(3))
                },
            )
            .returning(|id, _, _| {
                let mut row = case_row(4);
                row.id = id.to_owned();
                row.status = "APPROVED".to_owned();
                Ok(row)
            });

        let api = api_with(repo);
        let body = SubmitStepRequest {
            input: valid_input("APPROVED"),
        };
        let patch = apply_recovery_decision(&api, &flow_row(), &body)
            .await
            .expect("approval succeeds");
        let patch = patch.expect("recovery patch returned");
        assert_eq!(patch["recovery"]["case_version"], 4);
    }

    #[tokio::test]
    async fn non_recovery_flow_is_not_treated_as_decision() {
        let api = api_with(MockRecoveryCaseRepo::new());
        let mut f = flow_row();
        f.flow_type = "phone_otp".to_owned();
        let body = SubmitStepRequest {
            input: valid_input("APPROVED"),
        };
        let patch = apply_recovery_decision(&api, &f, &body)
            .await
            .expect("non-recovery flows pass through");
        assert!(patch.is_none());
    }
}
