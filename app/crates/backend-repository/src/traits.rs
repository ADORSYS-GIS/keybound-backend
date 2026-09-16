use chrono::{DateTime, Utc};
use serde_json::Value;

pub type RepoResult<T> = backend_core::Result<T>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageFilter {
    pub page: i32,
    pub limit: i32,
}

impl PageFilter {
    pub fn normalized(self) -> Self {
        Self {
            page: self.page.max(1),
            limit: self.limit.clamp(1, 100),
        }
    }

    pub fn offset(&self) -> i64 {
        i64::from((self.page - 1) * self.limit)
    }
}

#[derive(Debug, Clone)]
pub struct FlowSessionCreateInput {
    pub id: String,
    pub human_id: String,
    pub user_id: Option<String>,
    pub session_type: String,
    pub status: String,
    pub context: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowSessionFilter {
    pub user_id: Option<String>,
    pub user_ids: Option<Vec<String>>,
    pub session_type: Option<String>,
    pub status: Option<String>,
    pub page: i32,
    pub limit: i32,
}

impl FlowSessionFilter {
    pub fn normalized(self) -> Self {
        let page = self.page.max(1);
        let limit = self.limit.clamp(1, 100);
        let user_id = self
            .user_id
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty());
        let user_ids = self
            .user_ids
            .map(|values| {
                let mut normalized: Vec<String> = values
                    .into_iter()
                    .map(|value| value.trim().to_owned())
                    .filter(|value| !value.is_empty())
                    .collect();
                normalized.sort();
                normalized.dedup();
                normalized
            })
            .filter(|values| !values.is_empty());
        let session_type = self
            .session_type
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty());
        let status = self
            .status
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty());

        Self {
            user_id,
            user_ids,
            session_type,
            status,
            page,
            limit,
        }
    }

    pub fn offset(&self) -> i64 {
        i64::from((self.page - 1) * self.limit)
    }
}

#[derive(Debug, Clone)]
pub struct FlowInstanceCreateInput {
    pub id: String,
    pub human_id: String,
    pub session_id: String,
    pub flow_type: String,
    pub status: String,
    pub current_step: Option<String>,
    pub step_ids: Value,
    pub context: Value,
}

#[derive(Debug, Clone)]
pub struct FlowStepCreateInput {
    pub id: String,
    pub human_id: String,
    pub flow_id: String,
    pub step_type: String,
    pub actor: String,
    pub status: String,
    pub attempt_no: i32,
    pub input: Option<Value>,
    pub output: Option<Value>,
    pub error: Option<Value>,
    pub next_retry_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Default)]
pub struct FlowStepPatch {
    pub status: Option<String>,
    pub attempt_no: Option<i32>,
    pub input: Option<Option<Value>>,
    pub output: Option<Option<Value>>,
    pub error: Option<Option<Value>>,
    pub next_retry_at: Option<Option<DateTime<Utc>>>,
    pub finished_at: Option<Option<DateTime<Utc>>>,
}

impl FlowStepPatch {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn status(mut self, status: impl Into<String>) -> Self {
        self.status = Some(status.into());
        self
    }

    pub fn attempt_no(mut self, attempt_no: i32) -> Self {
        self.attempt_no = Some(attempt_no);
        self
    }

    pub fn input(mut self, input: Value) -> Self {
        self.input = Some(Some(input));
        self
    }

    pub fn output(mut self, output: Value) -> Self {
        self.output = Some(Some(output));
        self
    }

    pub fn error(mut self, error: Value) -> Self {
        self.error = Some(Some(error));
        self
    }

    pub fn clear_error(mut self) -> Self {
        self.error = Some(None);
        self
    }

    pub fn next_retry_at(mut self, next_retry_at: DateTime<Utc>) -> Self {
        self.next_retry_at = Some(Some(next_retry_at));
        self
    }

    pub fn finished_at(mut self, finished_at: DateTime<Utc>) -> Self {
        self.finished_at = Some(Some(finished_at));
        self
    }
}

#[derive(Debug, Clone)]
pub struct SigningKeyCreateInput {
    pub kid: String,
    pub private_key_pem: String,
    pub public_key_jwk: Value,
    pub algorithm: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub is_active: bool,
}

#[derive(Debug, Clone)]
pub struct UserDataUpsertInput {
    pub user_id: String,
    pub name: String,
    pub data_type: String,
    pub content: Value,
    pub eager_fetch: bool,
}

#[derive(Debug, Clone)]
pub struct DepositRecipientUpsertInput {
    pub provider: String,
    pub full_name: String,
    pub phone_number: String,
    pub phone_regex: String,
    pub currency: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DepositRecipientContact {
    pub provider: String,
    pub full_name: String,
    pub phone_number: String,
    pub currency: String,
}

#[backend_core::async_trait]
pub trait FlowRepo: Send + Sync {
    async fn create_session(
        &self,
        input: FlowSessionCreateInput,
    ) -> RepoResult<backend_model::db::FlowSessionRow>;

    async fn get_session(
        &self,
        session_id: &str,
    ) -> RepoResult<Option<backend_model::db::FlowSessionRow>>;

    async fn list_sessions(
        &self,
        filter: FlowSessionFilter,
    ) -> RepoResult<(Vec<backend_model::db::FlowSessionRow>, i64)>;

    async fn update_session_status(
        &self,
        session_id: &str,
        status: &str,
        completed_at: Option<DateTime<Utc>>,
    ) -> RepoResult<()>;

    async fn update_session_context(&self, session_id: &str, context: Value) -> RepoResult<()>;

    async fn create_flow(
        &self,
        input: FlowInstanceCreateInput,
    ) -> RepoResult<backend_model::db::FlowInstanceRow>;

    async fn get_flow(
        &self,
        flow_id: &str,
    ) -> RepoResult<Option<backend_model::db::FlowInstanceRow>>;

    async fn list_flows_for_session(
        &self,
        session_id: &str,
    ) -> RepoResult<Vec<backend_model::db::FlowInstanceRow>>;

    async fn update_flow(
        &self,
        flow_id: &str,
        status: Option<String>,
        current_step: Option<Option<String>>,
        step_ids: Option<Value>,
        context: Option<Value>,
    ) -> RepoResult<backend_model::db::FlowInstanceRow>;

    async fn create_step(
        &self,
        input: FlowStepCreateInput,
    ) -> RepoResult<backend_model::db::FlowStepRow>;

    async fn get_step(&self, step_id: &str) -> RepoResult<Option<backend_model::db::FlowStepRow>>;

    async fn list_steps_for_flow(
        &self,
        flow_id: &str,
    ) -> RepoResult<Vec<backend_model::db::FlowStepRow>>;

    async fn patch_step(
        &self,
        step_id: &str,
        patch: FlowStepPatch,
    ) -> RepoResult<backend_model::db::FlowStepRow>;

    async fn deactivate_signing_keys(&self) -> RepoResult<usize>;

    async fn create_signing_key(
        &self,
        input: SigningKeyCreateInput,
    ) -> RepoResult<backend_model::db::SigningKeyRow>;

    async fn get_active_signing_key(&self) -> RepoResult<Option<backend_model::db::SigningKeyRow>>;

    async fn list_active_signing_keys(&self) -> RepoResult<Vec<backend_model::db::SigningKeyRow>>;

    async fn claim_next_system_step(&self) -> RepoResult<Option<backend_model::db::FlowStepRow>>;
}

#[backend_core::async_trait]
pub trait UserRepo: Send + Sync {
    async fn create_user(
        &self,
        req: &backend_model::kc::UserUpsert,
    ) -> RepoResult<backend_model::db::UserRow>;

    async fn get_user(&self, user_id: &str) -> RepoResult<Option<backend_model::db::UserRow>>;

    async fn update_user(
        &self,
        user_id: &str,
        req: &backend_model::kc::UserUpsert,
    ) -> RepoResult<Option<backend_model::db::UserRow>>;

    async fn delete_user(&self, user_id: &str) -> RepoResult<u64>;

    async fn search_users(
        &self,
        req: &backend_model::kc::UserSearch,
    ) -> RepoResult<Vec<backend_model::db::UserRow>>;

    async fn resolve_user_by_phone(
        &self,
        realm: &str,
        phone: &str,
    ) -> RepoResult<Option<backend_model::db::UserRow>>;

    async fn find_users_by_phone(
        &self,
        realm: Option<String>,
        phone: &str,
    ) -> RepoResult<Vec<backend_model::db::UserRow>>;

    async fn resolve_or_create_user_by_phone(
        &self,
        realm: &str,
        phone: &str,
    ) -> RepoResult<(backend_model::db::UserRow, bool)>;

    async fn upsert_user_data(
        &self,
        input: UserDataUpsertInput,
    ) -> RepoResult<backend_model::db::UserDataRow>;

    async fn list_user_data(
        &self,
        user_id: &str,
        eager_fetch_only: bool,
    ) -> RepoResult<Vec<backend_model::db::UserDataRow>>;

    async fn update_phone_number(&self, user_id: &str, phone_number: &str) -> RepoResult<()>;
    async fn update_full_name(&self, user_id: &str, full_name: &str) -> RepoResult<()>;
    async fn get_user_metadata(&self, user_id: &str) -> RepoResult<Value>;

    /// Loads only the whitelisted metadata fields (by `name`) for a set of
    /// users in a single query. Used by least-privilege directory lookups so
    /// callers never aggregate arbitrary user metadata.
    async fn get_metadata_fields_for_users(
        &self,
        user_ids: Vec<String>,
        names: Vec<String>,
    ) -> RepoResult<std::collections::HashMap<String, Value>>;

    async fn update_metadata(
        &self,
        user_id: &str,
        metadata_patch: Value,
        eager_patch: Option<Value>,
    ) -> RepoResult<()>;
}

#[backend_core::async_trait]
pub trait DeviceRepo: Send + Sync {
    async fn lookup_device(
        &self,
        req: &backend_model::kc::DeviceLookupRequest,
    ) -> RepoResult<Option<backend_model::db::DeviceRow>>;

    async fn list_user_devices(
        &self,
        user_id: &str,
        include_revoked: bool,
    ) -> RepoResult<Vec<backend_model::db::DeviceRow>>;

    async fn get_user_device(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> RepoResult<Option<backend_model::db::DeviceRow>>;

    async fn update_device_status(
        &self,
        record_id: &str,
        status: &str,
    ) -> RepoResult<backend_model::db::DeviceRow>;

    async fn find_device_binding(
        &self,
        device_id: &str,
        jkt: &str,
    ) -> RepoResult<Option<(String, String)>>;

    async fn bind_device(
        &self,
        req: &backend_model::kc::EnrollmentBindRequest,
    ) -> RepoResult<String>;

    async fn count_user_devices(&self, user_id: &str) -> RepoResult<i64>;

    async fn find_recovery_idempotency(
        &self,
        idempotency_key: &str,
    ) -> RepoResult<Option<backend_model::db::RecoveryIdempotencyRow>>;

    async fn find_recovery_bind_by_case(
        &self,
        recovery_case_id: &str,
    ) -> RepoResult<Option<backend_model::db::RecoveryIdempotencyRow>>;

    async fn bind_recovery_device(
        &self,
        idempotency_key: &str,
        recovery_case_id: &str,
        request_hash: &str,
        req: &backend_model::kc::RecoveryBindRequest,
    ) -> RepoResult<String>;

    async fn find_old_device_policy_idempotency(
        &self,
        idempotency_key: &str,
    ) -> RepoResult<Option<backend_model::db::OldDevicePolicyIdempotencyRow>>;

    /// Applies the authoritative old-device policy for a recovery case.
    ///
    /// The target user is derived from the authoritative recovery bind record
    /// (never from caller-supplied identity). Returns the affected device
    /// record ids in the order they were updated, and whether the policy was
    /// already applied on a prior call (idempotent retry).
    async fn apply_old_device_policy(
        &self,
        idempotency_key: &str,
        recovery_case_id: &str,
        request_hash: &str,
        target_user_id: &str,
        policy: &str,
        except_device_ids: &[String],
    ) -> RepoResult<OldDevicePolicyOutcome>;
}

/// Result of applying an old-device policy for a recovery case.
#[derive(Debug, Clone)]
pub struct OldDevicePolicyOutcome {
    pub already_applied: bool,
    pub affected_device_ids: Vec<String>,
}

/// Input for creating a recovery-case aggregate row.
#[derive(Debug, Clone)]
pub struct RecoveryCaseCreateInput {
    pub id: String,
    pub human_id: String,
    pub session_id: Option<String>,
    pub device_id: Option<String>,
    pub jkt: Option<String>,
    pub device_public_jwk: Option<Value>,
    pub requested_phone_hash: String,
    pub requested_phone_masked: String,
    pub reason: Option<String>,
    pub status: String,
    pub phone_relation: Option<String>,
    pub matched_user_id: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
}

/// A partial, versioned update for a recovery-case row. Any `Some` field is
/// applied; `version` is used for optimistic concurrency: the update only
/// applies when the stored row's `version` equals `expected_version`, and
/// increments the stored `version` by one. A mismatch returns a conflict.
#[derive(Debug, Clone, Default)]
pub struct RecoveryCaseUpdate {
    pub session_id: Option<Option<String>>,
    pub requested_phone_hash: Option<String>,
    pub requested_phone_masked: Option<String>,
    pub device_id: Option<Option<String>>,
    pub jkt: Option<Option<String>>,
    pub device_public_jwk: Option<Option<Value>>,
    pub reason: Option<Option<String>>,
    pub status: Option<String>,
    pub phone_relation: Option<Option<String>>,
    pub matched_user_id: Option<Option<String>>,
    pub otp_hash: Option<Option<String>>,
    pub otp_expires_at: Option<Option<DateTime<Utc>>>,
    pub otp_attempts: Option<i32>,
    pub otp_resend_at: Option<Option<DateTime<Utc>>>,
    pub review_decision: Option<Option<String>>,
    pub review_reason: Option<Option<String>>,
    pub review_checklist: Option<Option<Value>>,
    pub review_expected_version: Option<Option<i64>>,
    pub approval_revision: Option<Option<i64>>,
    pub evidence: Option<Value>,
    pub old_devices: Option<Value>,
    pub risk_flags: Option<Value>,
    pub expires_at: Option<Option<DateTime<Utc>>>,
    pub review_expires_at: Option<Option<DateTime<Utc>>>,
    pub approved_expires_at: Option<Option<DateTime<Utc>>>,
}

impl RecoveryCaseUpdate {
    pub fn is_noop(&self) -> bool {
        self.session_id.is_none()
            && self.requested_phone_hash.is_none()
            && self.requested_phone_masked.is_none()
            && self.device_id.is_none()
            && self.jkt.is_none()
            && self.device_public_jwk.is_none()
            && self.reason.is_none()
            && self.status.is_none()
            && self.phone_relation.is_none()
            && self.matched_user_id.is_none()
            && self.otp_hash.is_none()
            && self.otp_expires_at.is_none()
            && self.otp_attempts.is_none()
            && self.otp_resend_at.is_none()
            && self.review_decision.is_none()
            && self.review_reason.is_none()
            && self.review_checklist.is_none()
            && self.review_expected_version.is_none()
            && self.approval_revision.is_none()
            && self.evidence.is_none()
            && self.old_devices.is_none()
            && self.risk_flags.is_none()
            && self.expires_at.is_none()
            && self.review_expires_at.is_none()
            && self.approved_expires_at.is_none()
    }
}

#[backend_core::async_trait]
pub trait RecoveryCaseRepo: Send + Sync {
    async fn create_case(
        &self,
        input: RecoveryCaseCreateInput,
    ) -> RepoResult<backend_model::db::RecoveryCaseRow>;

    async fn get_case_by_id(
        &self,
        case_id: &str,
    ) -> RepoResult<Option<backend_model::db::RecoveryCaseRow>>;

    async fn get_case_by_human_id(
        &self,
        human_id: &str,
    ) -> RepoResult<Option<backend_model::db::RecoveryCaseRow>>;

    async fn get_case_by_session_id(
        &self,
        session_id: &str,
    ) -> RepoResult<Option<backend_model::db::RecoveryCaseRow>>;

    async fn get_case_by_phone_hash(
        &self,
        phone_hash: &str,
    ) -> RepoResult<Option<backend_model::db::RecoveryCaseRow>>;

    /// Applies a versioned update. Returns `Err(Conflict)` when the stored
    /// `version` does not match `expected_version`.
    async fn update_case(
        &self,
        case_id: &str,
        expected_version: i64,
        patch: &RecoveryCaseUpdate,
    ) -> RepoResult<backend_model::db::RecoveryCaseRow>;

    async fn list_cases(
        &self,
        filter: RecoveryCaseFilter,
    ) -> RepoResult<(Vec<backend_model::db::RecoveryCaseRow>, i64)>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryCaseFilter {
    pub status: Option<String>,
    pub matched_user_id: Option<String>,
    pub page: i32,
    pub limit: i32,
}

impl RecoveryCaseFilter {
    pub fn normalized(self) -> Self {
        Self {
            status: self
                .status
                .map(|v| v.trim().to_owned())
                .filter(|v| !v.is_empty()),
            matched_user_id: self
                .matched_user_id
                .map(|v| v.trim().to_owned())
                .filter(|v| !v.is_empty()),
            page: self.page.max(1),
            limit: self.limit.clamp(1, 100),
        }
    }

    pub fn offset(&self) -> i64 {
        i64::from((self.page - 1) * self.limit)
    }
}
