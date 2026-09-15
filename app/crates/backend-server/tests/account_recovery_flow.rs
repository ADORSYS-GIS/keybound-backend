//! End-to-end test for the account_recovery flow definition.
//!
//! Loads the actual `flows/account_recovery.yaml` (via a dedicated fixture
//! directory so other flows with custom actions are not loaded), then drives
//! the first coherent slice: resolve_existing_account -> issue_recovery_otp ->
//! verify_recovery_otp -> await_admin_decision.

use async_trait::async_trait;
use backend_flow_sdk::{
    Actor, StepContext, StepOutcome, StepServices, UserLookupService, UserRecord,
};
use backend_server::flow_registry::{self, RegistryImports};
use serde_json::{Value, json};
use std::sync::Arc;

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");

fn fixture_registry() -> backend_flow_sdk::FlowRegistry {
    flow_registry::build_registry(RegistryImports {
        flows_dir: Some(format!("{FIXTURES}/flows")),
        sessions_dir: Some(format!("{FIXTURES}/sessions")),
        ..Default::default()
    })
    .expect("registry builds from account_recovery fixtures")
}

/// Test lookup service: only returns a match for the seeded phone, mimicking
/// the enumeration-safe find_users_by_phone behavior.
#[derive(Debug)]
struct TestUserLookup {
    known_phone: String,
    user_id: String,
}

#[async_trait]
impl UserLookupService for TestUserLookup {
    async fn get_user(&self, _user_id: &str) -> Result<Option<UserRecord>, String> {
        Ok(None)
    }

    async fn find_users_by_phone(
        &self,
        _realm: Option<String>,
        phone: &str,
    ) -> Result<Vec<UserRecord>, String> {
        if phone == self.known_phone {
            Ok(vec![UserRecord {
                user_id: self.user_id.clone(),
                realm: "fineract".to_string(),
                username: "seed".to_string(),
                full_name: Some("Seed User".to_string()),
                email: None,
                phone_number: Some(phone.to_string()),
                metadata: json!({}),
            }])
        } else {
            Ok(Vec::new())
        }
    }
}

fn merge_into(base: &mut Value, patch: &Value) {
    match (base, patch) {
        (Value::Object(b), Value::Object(p)) => {
            for (k, v) in p {
                if v.is_null() {
                    b.remove(k);
                } else if let Some(existing) = b.get_mut(k) {
                    merge_into(existing, v);
                } else {
                    b.insert(k.clone(), v.clone());
                }
            }
        }
        (slot, v) => *slot = v.clone(),
    }
}

fn services(lookup: Arc<TestUserLookup>) -> StepServices {
    StepServices {
        user_lookup: Some(lookup),
        ..Default::default()
    }
}

fn ctx(
    flow_id: &str,
    session_context: Value,
    flow_context: Value,
    services: StepServices,
) -> StepContext {
    StepContext {
        session_id: "sess_recovery".to_string(),
        session_user_id: None,
        flow_id: flow_id.to_string(),
        step_id: "step".to_string(),
        input: json!({}),
        session_context,
        flow_context,
        services,
    }
}

#[tokio::test]
async fn account_recovery_flow_advances_first_slice() {
    let registry = fixture_registry();

    let flow = registry.get_flow("account_recovery").expect("flow present");
    assert_eq!(flow.initial_step(), "init_recovery");
    assert_eq!(flow.human_id(), "rc");

    let step_names: Vec<&str> = flow.steps().iter().map(|s| s.step_type()).collect();
    for required in [
        "init_recovery",
        "resolve_existing_account",
        "issue_recovery_otp",
        "verify_recovery_otp",
        "await_admin_decision",
    ] {
        assert!(
            step_names.contains(&required),
            "flow missing step {required}: {step_names:?}"
        );
    }

    let known_phone = "+237690000000";
    let lookup = Arc::new(TestUserLookup {
        known_phone: known_phone.to_string(),
        user_id: "usr_seed".to_string(),
    });

    // Session context carries the requested phone (populated by init_recovery).
    let session_context = json!({ "phone_number": known_phone, "realm": "fineract" });

    // 1. resolve_existing_account (SYSTEM)
    let resolve = flow
        .steps()
        .iter()
        .find(|s| s.step_type() == "resolve_existing_account")
        .expect("resolve step");
    let resolve_ctx = ctx(
        "flow_1",
        session_context.clone(),
        json!({}),
        services(lookup.clone()),
    );
    let resolve_outcome = resolve
        .execute(&resolve_ctx)
        .await
        .expect("resolve executes");
    let (resolve_branch, resolve_output, resolve_updates) = match resolve_outcome {
        StepOutcome::Branched {
            branch,
            output,
            updates,
        } => (branch, output, updates),
        other => panic!("expected branched resolve outcome, got {other:?}"),
    };
    assert_eq!(resolve_branch, "matched");
    let resolve_output = resolve_output.expect("resolve output");
    assert_eq!(resolve_output["matched"], true);
    assert_eq!(resolve_output["matched_user_id"], "usr_seed");
    assert!(resolve_output["requested_phone_masked"].as_str().is_some());
    assert!(resolve_output["requested_phone_hash"].as_str().is_some());

    // Apply resolve's flow-context patch so issue sees `recovery.matched`.
    let mut flow_context = json!({});
    if let Some(updates) = resolve_updates
        && let Some(patch) = updates.flow_context_patch
    {
        merge_into(&mut flow_context, &patch);
    }
    assert_eq!(flow_context["recovery"]["matched"], true);

    // 2. issue_recovery_otp (SYSTEM) — stores a HASH, not plaintext.
    let issue = flow
        .steps()
        .iter()
        .find(|s| s.step_type() == "issue_recovery_otp")
        .expect("issue step");
    let issue_ctx = ctx(
        "flow_1",
        session_context.clone(),
        flow_context.clone(),
        services(lookup.clone()),
    );
    let issue_outcome = issue.execute(&issue_ctx).await.expect("issue executes");
    let (issue_output, issue_updates) = match issue_outcome {
        StepOutcome::Done { output, updates } => (output, updates),
        other => panic!("expected done issue outcome, got {other:?}"),
    };
    let issue_output = issue_output.expect("issue output");
    assert_eq!(issue_output["otp_sent"], true);
    let plaintext = issue_output["otp"]
        .as_str()
        .expect("plaintext otp in output");

    // The plaintext OTP must NOT be persisted into flow context.
    if let Some(updates) = &issue_updates
        && let Some(patch) = &updates.flow_context_patch
    {
        let serialized = patch.to_string();
        assert!(
            !serialized.contains(plaintext),
            "plaintext OTP leaked into flow context: {serialized}"
        );
    }

    // Apply issue's patch; verify the hash is stored and attempts reset.
    if let Some(updates) = issue_updates
        && let Some(patch) = updates.flow_context_patch
    {
        merge_into(&mut flow_context, &patch);
    }
    let stored_hash = flow_context["recovery"]["otp_hash"]
        .as_str()
        .expect("otp hash stored");
    assert_ne!(stored_hash, plaintext, "OTP must be stored hashed");
    assert_eq!(flow_context["recovery"]["otp_attempts"], 0);

    // 3. verify_recovery_otp (END_USER) — submit correct code via verify_input.
    let verify = flow
        .steps()
        .iter()
        .find(|s| s.step_type() == "verify_recovery_otp")
        .expect("verify step");
    assert_eq!(verify.actor(), Actor::EndUser);

    let verify_ctx = ctx(
        "flow_1",
        session_context.clone(),
        flow_context.clone(),
        services(lookup.clone()),
    );
    let verify_outcome = verify
        .verify_input(&verify_ctx, &json!({ "code": plaintext }))
        .await
        .expect("verify executes");
    match verify_outcome {
        StepOutcome::Branched { branch, output, .. } => {
            assert_eq!(branch, "verified");
            assert_eq!(output.expect("output")["verified"], true);
        }
        other => panic!("expected verified branch, got {other:?}"),
    }

    // 4. await_admin_decision (ADMIN) is a WAIT step in the flow.
    let await_step = flow
        .steps()
        .iter()
        .find(|s| s.step_type() == "await_admin_decision")
        .expect("await step");
    assert_eq!(await_step.actor(), Actor::Admin);
    let await_ctx = ctx("flow_1", session_context, flow_context, services(lookup));
    match await_step
        .execute(&await_ctx)
        .await
        .expect("await executes")
    {
        StepOutcome::Waiting { actor } => assert_eq!(actor, Actor::Admin),
        other => panic!("expected Waiting for admin, got {other:?}"),
    }
}

#[tokio::test]
async fn resolve_unknown_phone_does_not_leak_existence() {
    let registry = fixture_registry();
    let flow = registry.get_flow("account_recovery").expect("flow present");

    let lookup = Arc::new(TestUserLookup {
        known_phone: "+237690000000".to_string(),
        user_id: "usr_seed".to_string(),
    });

    // Unknown phone -> no_match branch, no user_id surfaced.
    let resolve = flow
        .steps()
        .iter()
        .find(|s| s.step_type() == "resolve_existing_account")
        .expect("resolve step");
    let ctx = ctx(
        "flow_2",
        json!({ "phone_number": "+237699999999", "realm": "fineract" }),
        json!({}),
        services(lookup),
    );
    let outcome = resolve.execute(&ctx).await.expect("resolve executes");
    let (branch, output) = match outcome {
        StepOutcome::Branched { branch, output, .. } => (branch, output),
        other => panic!("expected branched outcome, got {other:?}"),
    };
    assert_eq!(branch, "no_match");
    let output = output.expect("output");
    assert_eq!(output["matched"], false);
    assert!(output["matched_user_id"].is_null());

    let serialized = output.to_string();
    for forbidden in ["usr_seed", "Seed User", "+237699999999"] {
        assert!(
            !serialized.contains(forbidden),
            "no_match output leaked {forbidden}: {serialized}"
        );
    }
}
