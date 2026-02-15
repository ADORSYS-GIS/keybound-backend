use crate::pg::{PgRepository, PgSqlRepo};
use crate::traits::*;
use backend_model::db;

impl PgRepository {
    pub async fn queue_sms(&self, sms: SmsPendingInsert) -> RepoResult<SmsQueued> {
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

    pub async fn get_sms_by_hash(&self, hash: &str) -> RepoResult<Option<db::SmsMessageRow>> {
        let row = self.get_sms_by_hash_db(hash.to_owned()).await?;
        Ok(row)
    }

    pub async fn mark_sms_confirmed(&self, hash: &str) -> RepoResult<()> {
        self.mark_sms_confirmed_db(hash.to_owned()).await?;
        Ok(())
    }

    pub async fn list_retryable_sms(&self, limit: i64) -> RepoResult<Vec<db::SmsMessageRow>> {
        let rows = self.list_retryable_sms_db(limit).await?;
        Ok(rows)
    }

    pub async fn mark_sms_sent(&self, id: &str, sns_message_id: Option<String>) -> RepoResult<()> {
        self.mark_sms_sent_db(id.to_owned(), sns_message_id).await?;
        Ok(())
    }

    pub async fn mark_sms_failed(&self, update: SmsPublishFailure) -> RepoResult<()> {
        let status = if update.gave_up {
            "GAVE_UP".to_owned()
        } else {
            "FAILED".to_owned()
        };

        self.mark_sms_failed_db(update.id, status, update.error, update.next_retry_at)
            .await?;
        Ok(())
    }

    pub async fn mark_sms_gave_up(&self, id: &str, reason: &str) -> RepoResult<()> {
        self.mark_sms_gave_up_db(id.to_owned(), reason.to_owned())
            .await?;
        Ok(())
    }
}
