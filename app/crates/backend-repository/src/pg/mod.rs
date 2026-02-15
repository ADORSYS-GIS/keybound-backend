use backend_model::db;
use lru::LruCache;
use sqlx::PgPool;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};

mod approval;
mod device;
mod kyc;
mod sms;
mod user;

#[derive(Clone)]
pub struct PgRepository {
    pub(crate) pool: PgPool,
    pub(crate) resolve_user_by_phone_cache: Arc<Mutex<LruCache<String, Option<db::UserRow>>>>,
}

impl PgRepository {
    pub fn new(pool: PgPool) -> Self {
        let capacity = NonZeroUsize::new(50_000).expect("non-zero LRU capacity");
        let resolve_user_by_phone_cache = Arc::new(Mutex::new(LruCache::new(capacity)));

        Self {
            pool,
            resolve_user_by_phone_cache,
        }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub(crate) fn phone_cache_key(realm: &str, phone: &str) -> String {
        format!("{realm}:{phone}")
    }
}

use sqlx_data::{IntoParams, QueryResult, Serial, dml, repo};
use chrono::{DateTime, Utc};
use serde_json::Value;

#[repo]
pub(crate) trait PgSqlRepo {
    // KYC
    #[dml(file = "queries/bff/list_kyc_documents.sql", unchecked)]
    async fn list_kyc_documents_db(
        &self,
        external_id: String,
        params: impl IntoParams,
    ) -> sqlx_data::Result<Serial<db::KycDocumentRow>>;

    #[dml(file = "queries/staff/list_kyc_submissions.sql", unchecked)]
    async fn list_kyc_submissions_db(
        &self,
        params: impl IntoParams,
    ) -> sqlx_data::Result<Serial<db::KycProfileRow>>;

    #[dml(file = "queries/kyc/ensure_profile.sql", unchecked)]
    async fn ensure_kyc_profile_db(&self, external_id: String) -> sqlx_data::Result<QueryResult>;

    #[dml(file = "queries/kyc/insert_document_intent.sql", unchecked)]
    async fn insert_kyc_document_intent_db(
        &self,
        id: String,
        external_id: String,
        document_type: String,
        file_name: String,
        mime_type: String,
        content_length: i64,
        s3_bucket: String,
        s3_key: String,
        presigned_expires_at: DateTime<Utc>,
    ) -> sqlx_data::Result<db::KycDocumentRow>;

    #[dml(file = "queries/kyc/get_profile.sql", unchecked)]
    async fn get_kyc_profile_db(
        &self,
        external_id: String,
    ) -> sqlx_data::Result<Option<db::KycProfileRow>>;

    #[dml(file = "queries/kyc/get_tier.sql", unchecked)]
    async fn get_kyc_tier_db(&self, external_id: String) -> sqlx_data::Result<Option<i32>>;

    #[dml(file = "queries/kyc/update_approved.sql", unchecked)]
    async fn update_kyc_approved_db(
        &self,
        external_id: String,
        new_tier: i32,
        notes: Option<String>,
    ) -> sqlx_data::Result<QueryResult>;

    #[dml(file = "queries/kyc/update_rejected.sql", unchecked)]
    async fn update_kyc_rejected_db(
        &self,
        external_id: String,
        reason: String,
        notes: Option<String>,
    ) -> sqlx_data::Result<QueryResult>;

    #[dml(file = "queries/kyc/update_request_info.sql", unchecked)]
    async fn update_kyc_request_info_db(
        &self,
        external_id: String,
        message: String,
    ) -> sqlx_data::Result<QueryResult>;

    #[dml(file = "queries/kyc/patch_information.sql", unchecked)]
    async fn patch_kyc_information_db(
        &self,
        external_id: String,
        first_name: Option<String>,
        last_name: Option<String>,
        email: Option<String>,
        phone_number: Option<String>,
        date_of_birth: Option<String>,
        nationality: Option<String>,
    ) -> sqlx_data::Result<Option<db::KycProfileRow>>;

    // User
    #[dml(file = "queries/user/create.sql", unchecked)]
    async fn create_user_db(
        &self,
        user_id: String,
        realm: String,
        username: String,
        first_name: Option<String>,
        last_name: Option<String>,
        email: Option<String>,
        enabled: bool,
        email_verified: bool,
        attributes: Option<Value>,
    ) -> sqlx_data::Result<db::UserRow>;

    #[dml(file = "queries/user/get.sql", unchecked)]
    async fn get_user_db(&self, user_id: String) -> sqlx_data::Result<Option<db::UserRow>>;

    #[dml(file = "queries/user/update.sql", unchecked)]
    async fn update_user_db(
        &self,
        user_id: String,
        realm: String,
        username: String,
        first_name: Option<String>,
        last_name: Option<String>,
        email: Option<String>,
        enabled: bool,
        email_verified: bool,
        attributes: Option<Value>,
    ) -> sqlx_data::Result<Option<db::UserRow>>;

    #[dml(file = "queries/user/delete.sql", unchecked)]
    async fn delete_user_db(&self, user_id: String) -> sqlx_data::Result<QueryResult>;

    #[dml(file = "queries/user/search.sql", unchecked)]
    async fn search_users_db(
        &self,
        realm: String,
        search: Option<String>,
        username: Option<String>,
        email: Option<String>,
        enabled: Option<bool>,
        email_verified: Option<bool>,
        limit: i32,
        offset: i32,
    ) -> sqlx_data::Result<Vec<db::UserRow>>;

    #[dml(file = "queries/user/resolve_by_phone.sql", unchecked)]
    async fn resolve_user_by_phone_db(
        &self,
        realm: String,
        phone: String,
    ) -> sqlx_data::Result<Option<db::UserRow>>;

    #[dml(file = "queries/user/create_by_phone.sql", unchecked)]
    async fn create_user_by_phone_db(
        &self,
        user_id: String,
        realm: String,
        phone: String,
        attributes: Value,
    ) -> sqlx_data::Result<db::UserRow>;

    // Device
    #[dml(file = "queries/device/lookup.sql", unchecked)]
    async fn lookup_device_db(
        &self,
        device_id: Option<String>,
        jkt: Option<String>,
    ) -> sqlx_data::Result<Option<db::DeviceRow>>;

    #[dml(file = "queries/device/list_user_devices.sql", unchecked)]
    async fn list_user_devices_db(
        &self,
        user_id: String,
        include_revoked: bool,
    ) -> sqlx_data::Result<Vec<db::DeviceRow>>;

    #[dml(file = "queries/device/get_user_device.sql", unchecked)]
    async fn get_user_device_db(
        &self,
        user_id: String,
        device_id: String,
    ) -> sqlx_data::Result<Option<db::DeviceRow>>;

    #[dml(file = "queries/device/update_status.sql", unchecked)]
    async fn update_device_status_db(
        &self,
        record_id: String,
        status: String,
    ) -> sqlx_data::Result<db::DeviceRow>;

    #[dml(file = "queries/device/find_binding.sql", unchecked)]
    async fn find_device_binding_db(
        &self,
        device_id: String,
        jkt: String,
    ) -> sqlx_data::Result<Option<(String, String)>>;

    #[dml(file = "queries/device/bind.sql", unchecked)]
    async fn bind_device_db(
        &self,
        id: String,
        realm: String,
        client_id: String,
        user_id: String,
        user_hint: Option<String>,
        device_id: String,
        jkt: String,
        public_jwk: Value,
        attributes: Option<Value>,
        proof: Option<Value>,
    ) -> sqlx_data::Result<String>;

    #[dml(file = "queries/device/count_user_devices.sql", unchecked)]
    async fn count_user_devices_db(&self, user_id: String) -> sqlx_data::Result<i64>;

    // Approval
    #[dml(file = "queries/approval/create.sql", unchecked)]
    async fn create_approval_db(
        &self,
        request_id: String,
        realm: String,
        client_id: String,
        user_id: String,
        device_id: String,
        jkt: String,
        public_jwk: Option<Value>,
        platform: Option<String>,
        model: Option<String>,
        app_version: Option<String>,
        reason: Option<String>,
        expires_at: Option<DateTime<Utc>>,
        context: Option<Value>,
        idempotency_key: Option<String>,
    ) -> sqlx_data::Result<(String, String, Option<DateTime<Utc>>)>;

    #[dml(file = "queries/approval/get.sql", unchecked)]
    async fn get_approval_db(
        &self,
        request_id: String,
    ) -> sqlx_data::Result<Option<db::ApprovalRow>>;

    #[dml(file = "queries/approval/list_user_approvals.sql", unchecked)]
    async fn list_user_approvals_db(
        &self,
        user_id: String,
        statuses: Option<Vec<String>>,
    ) -> sqlx_data::Result<Vec<db::ApprovalRow>>;

    #[dml(file = "queries/approval/decide.sql", unchecked)]
    async fn decide_approval_db(
        &self,
        request_id: String,
        status: String,
        decided_by_device_id: Option<String>,
        message: Option<String>,
    ) -> sqlx_data::Result<Option<db::ApprovalRow>>;

    #[dml(file = "queries/approval/cancel.sql", unchecked)]
    async fn cancel_approval_db(&self, request_id: String) -> sqlx_data::Result<QueryResult>;

    // SMS
    #[dml(file = "queries/sms/queue.sql", unchecked)]
    async fn queue_sms_db(
        &self,
        id: String,
        realm: String,
        client_id: String,
        user_id: Option<String>,
        phone_number: String,
        hash: String,
        otp_sha256: Vec<u8>,
        ttl_seconds: i32,
        max_attempts: i32,
        metadata: Value,
    ) -> sqlx_data::Result<QueryResult>;

    #[dml(file = "queries/sms/get_by_hash.sql", unchecked)]
    async fn get_sms_by_hash_db(
        &self,
        hash: String,
    ) -> sqlx_data::Result<Option<db::SmsMessageRow>>;

    #[dml(file = "queries/sms/mark_confirmed.sql", unchecked)]
    async fn mark_sms_confirmed_db(&self, hash: String) -> sqlx_data::Result<QueryResult>;

    #[dml(file = "queries/sms/list_retryable.sql", unchecked)]
    async fn list_retryable_sms_db(&self, limit: i64) -> sqlx_data::Result<Vec<db::SmsMessageRow>>;

    #[dml(file = "queries/sms/mark_sent.sql", unchecked)]
    async fn mark_sms_sent_db(
        &self,
        id: String,
        sns_message_id: Option<String>,
    ) -> sqlx_data::Result<QueryResult>;

    #[dml(file = "queries/sms/mark_failed.sql", unchecked)]
    async fn mark_sms_failed_db(
        &self,
        id: String,
        status: String,
        error: String,
        next_retry_at: Option<DateTime<Utc>>,
    ) -> sqlx_data::Result<QueryResult>;

    #[dml(file = "queries/sms/mark_gave_up.sql", unchecked)]
    async fn mark_sms_gave_up_db(
        &self,
        id: String,
        reason: String,
    ) -> sqlx_data::Result<QueryResult>;
}

impl PgSqlRepo for PgRepository {
    fn get_pool(&self) -> &sqlx_data::Pool {
        &self.pool
    }
}
