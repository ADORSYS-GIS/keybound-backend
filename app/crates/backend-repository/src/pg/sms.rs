use crate::traits::*;
use backend_model::db;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;
use sqlx_data::{QueryResult, dml, repo};

#[repo]
pub trait PgSmsRepo {
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

#[derive(Clone)]
pub struct SmsRepository {
    pub(crate) pool: PgPool,
}

impl PgSmsRepo for SmsRepository {
    fn get_pool(&self) -> &sqlx_data::Pool {
        &self.pool
    }
}

impl SmsRepo for SmsRepository {
    async fn queue_sms(&self, sms: SmsPendingInsert) -> RepoResult<SmsQueued> {
        let hash = backend_id::sms_hash()?;
        self.queue_sms_db(
            hash.clone(),
            sms.realm,
            sms.client_id,
            sms.user_id,
            sms.phone_number,
            hash.clone(),
            sms.otp_sha256,
            sms.ttl_seconds,
            sms.max_attempts,
            sms.metadata,
        )
        .await?;

        Ok(SmsQueued {
            hash,
            ttl_seconds: sms.ttl_seconds,
            status: "PENDING".to_owned(),
        })
    }

    async fn get_sms_by_hash(&self, hash: &str) -> RepoResult<Option<db::SmsMessageRow>> {
        let row = self.get_sms_by_hash_db(hash.to_owned()).await?;
        Ok(row)
    }

    async fn mark_sms_confirmed(&self, hash: &str) -> RepoResult<()> {
        self.mark_sms_confirmed_db(hash.to_owned()).await?;
        Ok(())
    }

    async fn list_retryable_sms(&self, limit: i64) -> RepoResult<Vec<db::SmsMessageRow>> {
        let rows = self.list_retryable_sms_db(limit).await?;
        Ok(rows)
    }

    async fn mark_sms_sent(&self, id: &str, sns_message_id: Option<String>) -> RepoResult<()> {
        self.mark_sms_sent_db(id.to_owned(), sns_message_id).await?;
        Ok(())
    }

    async fn mark_sms_failed(&self, update: SmsPublishFailure) -> RepoResult<()> {
        let status = if update.gave_up {
            "GAVE_UP".to_owned()
        } else {
            "FAILED".to_owned()
        };

        self.mark_sms_failed_db(update.id, status, update.error, update.next_retry_at)
            .await?;
        Ok(())
    }

    async fn mark_sms_gave_up(&self, id: &str, reason: &str) -> RepoResult<()> {
        self.mark_sms_gave_up_db(id.to_owned(), reason.to_owned())
            .await?;
        Ok(())
    }
}

impl SmsRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}
