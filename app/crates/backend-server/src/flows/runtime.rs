use backend_flow_sdk::{
    Flow, RecoveryDeviceBindRequest, RecoveryDeviceService, StepServices, UserContactService,
    UserLookupService, UserRecord,
};
use backend_repository::UserRepo;
use serde_json::Value;
use std::sync::Arc;

pub struct RepoUserLookup {
    user_repo: Arc<dyn UserRepo>,
}

impl RepoUserLookup {
    pub fn new(user_repo: Arc<dyn UserRepo>) -> Self {
        Self { user_repo }
    }
}

pub struct RepoUserContact {
    user_repo: Arc<dyn UserRepo>,
}

impl RepoUserContact {
    pub fn new(user_repo: Arc<dyn UserRepo>) -> Self {
        Self { user_repo }
    }
}

impl std::fmt::Debug for RepoUserLookup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RepoUserLookup")
            .field("user_repo", &"<UserRepo>")
            .finish()
    }
}

impl std::fmt::Debug for RepoUserContact {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RepoUserContact")
            .field("user_repo", &"<UserRepo>")
            .finish()
    }
}

#[backend_core::async_trait]
impl UserLookupService for RepoUserLookup {
    async fn get_user(&self, user_id: &str) -> Result<Option<UserRecord>, String> {
        let user = self
            .user_repo
            .get_user(user_id)
            .await
            .map_err(|error| error.to_string())?;

        let Some(row) = user else {
            return Ok(None);
        };

        let metadata = self
            .user_repo
            .get_user_metadata(&row.user_id)
            .await
            .map_err(|error| error.to_string())?;

        Ok(Some(UserRecord {
            user_id: row.user_id,
            realm: row.realm,
            username: row.username,
            full_name: row.full_name,
            email: row.email,
            phone_number: row.phone_number,
            metadata,
        }))
    }

    async fn find_users_by_phone(
        &self,
        realm: Option<String>,
        phone: &str,
    ) -> Result<Vec<UserRecord>, String> {
        let rows = self
            .user_repo
            .find_users_by_phone(realm, phone)
            .await
            .map_err(|error| error.to_string())?;

        let mut records = Vec::with_capacity(rows.len());
        for row in rows {
            let metadata = self
                .user_repo
                .get_user_metadata(&row.user_id)
                .await
                .map_err(|error| error.to_string())?;
            records.push(UserRecord {
                user_id: row.user_id,
                realm: row.realm,
                username: row.username,
                full_name: row.full_name,
                email: row.email,
                phone_number: row.phone_number,
                metadata,
            });
        }

        Ok(records)
    }
}

#[backend_core::async_trait]
impl UserContactService for RepoUserContact {
    async fn update_phone_number(&self, user_id: &str, phone_number: &str) -> Result<(), String> {
        self.user_repo
            .update_phone_number(user_id, phone_number)
            .await
            .map_err(|error| error.to_string())
    }

    async fn update_full_name(&self, user_id: &str, full_name: &str) -> Result<(), String> {
        self.user_repo
            .update_full_name(user_id, full_name)
            .await
            .map_err(|error| error.to_string())
    }
}

pub fn step_services_with_device(
    user_repo: Arc<dyn UserRepo>,
    device_repo: Arc<dyn backend_repository::DeviceRepo>,
) -> StepServices {
    StepServices {
        user_lookup: Some(Arc::new(RepoUserLookup::new(user_repo.clone()))),
        user_contact: Some(Arc::new(RepoUserContact::new(user_repo.clone()))),
        recovery_device: Some(Arc::new(RepoRecoveryDevice::new(device_repo))),
        ..Default::default()
    }
}

pub struct RepoRecoveryDevice {
    device_repo: Arc<dyn backend_repository::DeviceRepo>,
}

impl RepoRecoveryDevice {
    pub fn new(device_repo: Arc<dyn backend_repository::DeviceRepo>) -> Self {
        Self { device_repo }
    }
}

impl std::fmt::Debug for RepoRecoveryDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RepoRecoveryDevice")
            .field("device_repo", &"<DeviceRepo>")
            .finish()
    }
}

#[backend_core::async_trait]
impl RecoveryDeviceService for RepoRecoveryDevice {
    async fn bind_recovery_device(
        &self,
        recovery_case_id: &str,
        req: RecoveryDeviceBindRequest,
    ) -> Result<backend_flow_sdk::RecoveryDeviceBindOutcome, String> {
        let domain_req = backend_model::kc::RecoveryBindRequest {
            realm: req.realm,
            target_user_id: req.target_user_id.clone(),
            approval_revision: req.approval_revision,
            device_id: req.device_id,
            jkt: req.jkt,
            public_jwk: req
                .public_jwk
                .as_object()
                .map(|map| {
                    map.iter()
                        .map(|(k, v)| (k.clone(), gen_oas_server_kc::types::Object(v.clone())))
                        .collect()
                })
                .unwrap_or_default(),
            binding_operation_id: req.binding_operation_id,
        };

        let idempotency_key = backend_id::flow_step_id().map_err(|e| e.to_string())?;
        let request_hash = recovery_case_id.to_string();

        let record_id = self
            .device_repo
            .bind_recovery_device(
                &idempotency_key,
                recovery_case_id,
                &request_hash,
                &domain_req,
            )
            .await
            .map_err(|error| error.to_string())?;

        Ok(backend_flow_sdk::RecoveryDeviceBindOutcome {
            device_record_id: record_id,
            bound_user_id: req.target_user_id,
        })
    }

    async fn apply_old_device_policy(
        &self,
        recovery_case_id: &str,
        realm: String,
        approval_revision: i64,
        policy: String,
        except_device_ids: Vec<String>,
    ) -> Result<Vec<String>, String> {
        let bind_record = self
            .device_repo
            .find_recovery_bind_by_case(recovery_case_id)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "RECOVERY_CASE_NOT_BOUND".to_string())?;

        let authoritative_except = vec![bind_record.device_id.clone()];
        let domain_req = backend_model::kc::OldDevicePolicyRequest {
            realm,
            approval_revision,
            policy,
            except_device_ids: authoritative_except,
            reason: None,
        };

        let idempotency_key = backend_id::flow_step_id().map_err(|e| e.to_string())?;
        let request_hash = recovery_case_id.to_string();

        let outcome = self
            .device_repo
            .apply_old_device_policy(
                &idempotency_key,
                recovery_case_id,
                &request_hash,
                &bind_record.bound_user_id,
                &domain_req.policy,
                &domain_req.except_device_ids,
            )
            .await
            .map_err(|error| error.to_string())?;

        Ok(outcome.affected_device_ids)
    }
}

pub fn merge_json_value(base: &mut Value, patch: &Value) {
    match (base, patch) {
        (Value::Object(base_obj), Value::Object(patch_obj)) => {
            for (key, value) in patch_obj {
                if value.is_null() {
                    base_obj.remove(key);
                    continue;
                }

                if let Some(existing) = base_obj.get_mut(key) {
                    merge_json_value(existing, value);
                } else {
                    base_obj.insert(key.clone(), value.clone());
                }
            }
        }
        (slot, value) => {
            *slot = value.clone();
        }
    }
}

pub fn merged_json(mut base: Value, patch: &Value) -> Value {
    merge_json_value(&mut base, patch);
    base
}

pub fn resolve_transition(
    flow: &dyn Flow,
    step_type: &str,
    branch: Option<&str>,
    failed: bool,
) -> Option<String> {
    let transition = flow.transitions().get(step_type)?;
    if let Some(branch_name) = branch
        && let Some(target) = transition.branches.get(branch_name)
    {
        return Some(target.clone());
    }

    if failed {
        return transition.on_failure.clone();
    }

    Some(transition.on_success.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use backend_flow_sdk::{Actor, Step, StepTransition};
    use std::collections::HashMap;

    struct TestStep;

    #[async_trait::async_trait]
    impl Step for TestStep {
        fn step_type(&self) -> &str {
            "start"
        }

        fn actor(&self) -> Actor {
            Actor::System
        }

        fn human_id(&self) -> &str {
            "start"
        }
    }

    struct TestFlow {
        transitions: HashMap<String, StepTransition>,
        steps: Vec<std::sync::Arc<dyn Step>>,
    }

    impl Flow for TestFlow {
        fn flow_type(&self) -> &str {
            "test"
        }

        fn human_id(&self) -> &str {
            "test"
        }

        fn feature(&self) -> Option<&str> {
            None
        }

        fn steps(&self) -> &[std::sync::Arc<dyn Step>] {
            &self.steps
        }

        fn initial_step(&self) -> &str {
            "start"
        }

        fn transitions(&self) -> &HashMap<String, StepTransition> {
            &self.transitions
        }
    }

    #[test]
    fn resolve_transition_prefers_named_branch() {
        let mut branches = HashMap::new();
        branches.insert("approved".to_owned(), "approve".to_owned());
        let flow = TestFlow {
            transitions: HashMap::from([(
                "start".to_owned(),
                StepTransition {
                    on_success: "next".to_owned(),
                    on_failure: Some("FAILED".to_owned()),
                    branches,
                },
            )]),
            steps: vec![std::sync::Arc::new(TestStep)],
        };

        assert_eq!(
            resolve_transition(&flow, "start", Some("approved"), false).as_deref(),
            Some("approve")
        );
        assert_eq!(
            resolve_transition(&flow, "start", None, true).as_deref(),
            Some("FAILED")
        );
    }

    #[test]
    fn resolve_transition_falls_back_to_success_branch() {
        let flow = TestFlow {
            transitions: HashMap::from([(
                "start".to_owned(),
                StepTransition {
                    on_success: "next".to_owned(),
                    on_failure: Some("FAILED".to_owned()),
                    branches: HashMap::new(),
                },
            )]),
            steps: vec![std::sync::Arc::new(TestStep)],
        };

        assert_eq!(
            resolve_transition(&flow, "start", Some("missing"), false).as_deref(),
            Some("next")
        );
    }

    #[tokio::test]
    async fn repo_user_lookup_find_by_phone_routes_to_repository() {
        use crate::test_utils::MockUserRepo;
        use backend_model::db::UserRow;

        let phone = "+237690000000";
        let row = UserRow {
            user_id: "usr-1".to_string(),
            realm: "fineract".to_string(),
            username: phone.to_string(),
            full_name: Some("Jane".to_string()),
            email: None,
            email_verified: false,
            phone_number: Some(phone.to_string()),
            disabled: false,
            attributes: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let mut repo = MockUserRepo::new();
        repo.expect_find_users_by_phone()
            .withf(move |realm: &Option<String>, p: &str| {
                realm.as_deref() == Some("fineract") && p == phone
            })
            .returning(move |_, _| Ok(vec![row.clone()]));
        repo.expect_get_user_metadata()
            .withf(|uid: &str| uid == "usr-1")
            .returning(|_| Ok(serde_json::json!({ "fineractClientId": "c-1" })));

        let lookup = RepoUserLookup::new(std::sync::Arc::new(repo));
        let found = lookup
            .find_users_by_phone(Some("fineract".to_string()), phone)
            .await
            .expect("lookup succeeds");

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].user_id, "usr-1");
        assert_eq!(found[0].realm, "fineract");
        assert_eq!(found[0].phone_number.as_deref(), Some(phone));
        assert_eq!(
            found[0]
                .metadata
                .get("fineractClientId")
                .and_then(serde_json::Value::as_str),
            Some("c-1")
        );
    }
}
