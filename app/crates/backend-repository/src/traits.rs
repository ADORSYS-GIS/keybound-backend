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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmInstanceFilter {
    pub kind: Option<String>,
    pub status: Option<String>,
    pub user_id: Option<String>,
    pub phone_number: Option<String>,
    pub created_from: Option<DateTime<Utc>>,
    pub created_to: Option<DateTime<Utc>>,
    pub page: i32,
    pub limit: i32,
}

impl SmInstanceFilter {
    pub fn normalized(self) -> Self {
        let page = self.page.max(1);
        let limit = self.limit.clamp(1, 100);
        let kind = self.kind.map(|v| v.trim().to_owned()).filter(|v| !v.is_empty());
        let status = self
            .status
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty());
        let user_id = self
            .user_id
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty());
        let phone_number = self
            .phone_number
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty());

        Self {
            kind,
            status,
            user_id,
            phone_number,
            created_from: self.created_from,
            created_to: self.created_to,
            page,
            limit,
        }
    }

    pub fn offset(&self) -> i64 {
        i64::from((self.page - 1) * self.limit)
    }
}

#[derive(Debug, Clone)]
pub struct SmInstanceCreateInput {
    pub id: String,
    pub kind: String,
    pub user_id: Option<String>,
    pub idempotency_key: String,
    pub status: String,
    pub context: Value,
}

#[derive(Debug, Clone)]
pub struct SmEventCreateInput {
    pub id: String,
    pub instance_id: String,
    pub kind: String,
    pub actor_type: String,
    pub actor_id: Option<String>,
    pub payload: Value,
}

#[derive(Debug, Clone)]
pub struct SmStepAttemptCreateInput {
    pub id: String,
    pub instance_id: String,
    pub step_name: String,
    pub attempt_no: i32,
    pub status: String,
    pub external_ref: Option<String>,
    pub input: Value,
    pub output: Option<Value>,
    pub error: Option<Value>,
    pub queued_at: Option<DateTime<Utc>>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub next_retry_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct SmStepAttemptPatch {
    pub status: Option<String>,
    pub output: Option<Option<Value>>,
    pub error: Option<Option<Value>>,
    pub queued_at: Option<Option<DateTime<Utc>>>,
    pub started_at: Option<Option<DateTime<Utc>>>,
    pub finished_at: Option<Option<DateTime<Utc>>>,
    pub next_retry_at: Option<Option<DateTime<Utc>>>,
}

#[backend_core::async_trait]
pub trait StateMachineRepo: Send + Sync {
    async fn create_instance(
        &self,
        input: SmInstanceCreateInput,
    ) -> RepoResult<backend_model::db::SmInstanceRow>;

    async fn get_instance(
        &self,
        instance_id: &str,
    ) -> RepoResult<Option<backend_model::db::SmInstanceRow>>;

    async fn get_instance_by_idempotency_key(
        &self,
        idempotency_key: &str,
    ) -> RepoResult<Option<backend_model::db::SmInstanceRow>>;

    async fn list_instances(
        &self,
        filter: SmInstanceFilter,
    ) -> RepoResult<(Vec<backend_model::db::SmInstanceRow>, i64)>;

    async fn update_instance_status(
        &self,
        instance_id: &str,
        status: &str,
        completed_at: Option<DateTime<Utc>>,
    ) -> RepoResult<()>;

    async fn update_instance_context(&self, instance_id: &str, context: Value) -> RepoResult<()>;

    async fn append_event(&self, input: SmEventCreateInput) -> RepoResult<backend_model::db::SmEventRow>;

    async fn list_events(&self, instance_id: &str) -> RepoResult<Vec<backend_model::db::SmEventRow>>;

    async fn create_step_attempt(
        &self,
        input: SmStepAttemptCreateInput,
    ) -> RepoResult<backend_model::db::SmStepAttemptRow>;

    async fn patch_step_attempt(
        &self,
        attempt_id: &str,
        patch: SmStepAttemptPatch,
    ) -> RepoResult<backend_model::db::SmStepAttemptRow>;

    /// Atomically claim a queued attempt for execution.
    /// Returns None if the attempt was not in QUEUED state (already running/finished/cancelled).
    async fn claim_step_attempt(
        &self,
        attempt_id: &str,
    ) -> RepoResult<Option<backend_model::db::SmStepAttemptRow>>;

    async fn list_step_attempts(
        &self,
        instance_id: &str,
    ) -> RepoResult<Vec<backend_model::db::SmStepAttemptRow>>;

    async fn get_latest_step_attempt(
        &self,
        instance_id: &str,
        step_name: &str,
    ) -> RepoResult<Option<backend_model::db::SmStepAttemptRow>>;

    async fn get_step_attempt_by_external_ref(
        &self,
        instance_id: &str,
        step_name: &str,
        external_ref: &str,
    ) -> RepoResult<Option<backend_model::db::SmStepAttemptRow>>;

    async fn cancel_other_attempts_for_step(
        &self,
        instance_id: &str,
        step_name: &str,
        keep_attempt_id: &str,
    ) -> RepoResult<()>;

    async fn next_attempt_no(&self, instance_id: &str, step_name: &str) -> RepoResult<i32>;

    async fn select_deposit_staff_contact(
        &self,
        user_id: &str,
    ) -> RepoResult<(String, String, String)>;
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
    async fn resolve_or_create_user_by_phone(
        &self,
        realm: &str,
        phone: &str,
    ) -> RepoResult<(backend_model::db::UserRow, bool)>;
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
}
