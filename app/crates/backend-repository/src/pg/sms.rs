use crate::traits::*;
use backend_model::db;
use chrono::Utc;
use diesel::prelude::*;
use diesel_async::AsyncPgConnection;
use diesel_async::RunQueryDsl;
use diesel_async::pooled_connection::deadpool::Pool;

#[derive(Clone)]
pub struct SmsRepository {
    pub(crate) pool: Pool<AsyncPgConnection>,
}

impl SmsRepository {
    pub fn new(pool: Pool<AsyncPgConnection>) -> Self {
        Self { pool }
    }

    async fn get_conn(
        &self,
    ) -> RepoResult<diesel_async::pooled_connection::deadpool::Object<AsyncPgConnection>> {
        self.pool
            .get()
            .await
            .map_err(|e| backend_core::Error::DieselPool(e.to_string()))
    }
}

impl SmsRepo for SmsRepository {
    async fn queue_sms(&self, sms: SmsPendingInsert) -> RepoResult<SmsQueued> {
        use backend_model::schema::sms_messages::dsl::*;

        let mut conn = self.get_conn().await?;
        let hash_val = backend_id::sms_hash()?;
        let id_val = backend_id::sms_id()?;

        let new_sms = db::SmsMessageRow {
            id: id_val,
            realm: sms.realm,
            client_id: sms.client_id,
            user_id: sms.user_id,
            phone_number: sms.phone_number,
            hash: hash_val.clone(),
            otp_sha256: sms.otp_sha256,
            ttl_seconds: sms.ttl_seconds,
            status: "PENDING".to_owned(),
            attempt_count: 0,
            max_attempts: sms.max_attempts,
            next_retry_at: Some(Utc::now()),
            last_error: None,
            sns_message_id: None,
            session_id: None,
            trace_id: None,
            metadata: Some(sms.metadata),
            created_at: Utc::now(),
            sent_at: None,
            confirmed_at: None,
        };

        diesel::insert_into(sms_messages)
            .values(&new_sms)
            .execute(&mut conn)
            .await
            .map_err(backend_core::Error::from)?;

        Ok(SmsQueued {
            hash: hash_val,
            ttl_seconds: sms.ttl_seconds,
            status: "PENDING".to_owned(),
        })
    }

    async fn get_sms_by_hash(&self, hash_val: &str) -> RepoResult<Option<db::SmsMessageRow>> {
        use backend_model::schema::sms_messages::dsl::*;

        let mut conn = self.get_conn().await?;

        sms_messages
            .filter(hash.eq(hash_val))
            .first::<db::SmsMessageRow>(&mut conn)
            .await
            .optional()
            .map_err(Into::into)
    }

    async fn mark_sms_confirmed(&self, hash_val: &str) -> RepoResult<()> {
        use backend_model::schema::sms_messages::dsl::*;

        let mut conn = self.get_conn().await?;

        diesel::update(sms_messages.filter(hash.eq(hash_val)))
            .set(confirmed_at.eq(Utc::now()))
            .execute(&mut conn)
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    async fn list_retryable_sms(&self, limit_val: i64) -> RepoResult<Vec<db::SmsMessageRow>> {
        use backend_model::schema::sms_messages::dsl::*;

        let mut conn = self.get_conn().await?;

        sms_messages
            .filter(status.eq_any(vec!["PENDING", "FAILED"]))
            .filter(next_retry_at.le(Utc::now()).or(next_retry_at.is_null()))
            .filter(attempt_count.lt(max_attempts))
            .order(created_at.asc())
            .limit(limit_val)
            .load::<db::SmsMessageRow>(&mut conn)
            .await
            .map_err(Into::into)
    }

    async fn mark_sms_sent(
        &self,
        id_val: &str,
        sns_message_id_val: Option<String>,
    ) -> RepoResult<()> {
        use backend_model::schema::sms_messages::dsl::*;

        let mut conn = self.get_conn().await?;

        diesel::update(sms_messages.filter(id.eq(id_val)))
            .set((
                status.eq("SENT"),
                sns_message_id.eq(sns_message_id_val),
                sent_at.eq(Utc::now()),
                attempt_count.eq(attempt_count + 1),
            ))
            .execute(&mut conn)
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    async fn mark_sms_failed(&self, update: SmsPublishFailure) -> RepoResult<()> {
        use backend_model::schema::sms_messages::dsl::*;

        let mut conn = self.get_conn().await?;

        let status_val = if update.gave_up { "GAVE_UP" } else { "FAILED" };

        diesel::update(sms_messages.filter(id.eq(update.id)))
            .set((
                status.eq(status_val),
                last_error.eq(update.error),
                next_retry_at.eq(update.next_retry_at),
                attempt_count.eq(attempt_count + 1),
            ))
            .execute(&mut conn)
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    async fn mark_sms_gave_up(&self, id_val: &str, reason: &str) -> RepoResult<()> {
        use backend_model::schema::sms_messages::dsl::*;

        let mut conn = self.get_conn().await?;

        diesel::update(sms_messages.filter(id.eq(id_val)))
            .set((status.eq("GAVE_UP"), last_error.eq(reason)))
            .execute(&mut conn)
            .await
            .map(|_| ())
            .map_err(Into::into)
    }
}
