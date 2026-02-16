use crate::traits::*;
use backend_model::{db, kc as kc_map};
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use diesel_async::pooled_connection::deadpool::Pool;
use diesel_async::AsyncPgConnection;
use sqlx::PgPool;

#[derive(Clone)]
pub struct ApprovalRepository {
    pub(crate) pool: PgPool,
    pub(crate) diesel_pool: Pool<AsyncPgConnection>,
}

impl ApprovalRepository {
    pub fn new(pool: PgPool, diesel_pool: Pool<AsyncPgConnection>) -> Self {
        Self { pool, diesel_pool }
    }

    async fn get_conn(
        &self,
    ) -> RepoResult<diesel_async::pooled_connection::deadpool::Object<AsyncPgConnection>> {
        self.diesel_pool
            .get()
            .await
            .map_err(|e| backend_core::Error::DieselPool(e.to_string()))
    }
}

impl ApprovalRepo for ApprovalRepository {
    async fn create_approval(
        &self,
        req: &kc_map::ApprovalCreateRequest,
        _idempotency_key_val: Option<String>,
    ) -> RepoResult<ApprovalCreated> {
        use backend_model::schema::approval::dsl::*;

        let request_id_val = backend_id::approval_id()?;
        let mut conn = self.get_conn().await?;

        let public_jwk_str = req
            .new_device
            .public_jwk
            .clone()
            .map(|m| serde_json::to_string(&m).unwrap_or_default())
            .unwrap_or_default();

        // Note: The original code used Value for context, but schema says Text for public_jwk and doesn't have context/idempotency_key in schema.rs
        // Wait, I should check the migration again.
        // Migration 20260203000007_device_approval_sms.sql:
        /*
        CREATE TABLE approval (
            request_id VARCHAR(40) NOT NULL,
            user_id VARCHAR(40) NOT NULL,
            new_device_id VARCHAR(40) NOT NULL,
            new_device_jkt VARCHAR(255) NOT NULL,
            new_device_public_jwk TEXT NOT NULL,
            new_device_platform VARCHAR(64),
            new_device_model VARCHAR(128),
            new_device_app_version VARCHAR(64),
            status VARCHAR(255) NOT NULL,
            created_at TIMESTAMP WITH TIME ZONE NOT NULL,
            expires_at TIMESTAMP WITH TIME ZONE NOT NULL,
            decided_at TIMESTAMP WITH TIME ZONE,
            decided_by_device_id VARCHAR(40),
            message VARCHAR(512),
            PRIMARY KEY (request_id),
            CONSTRAINT fk_approval_user FOREIGN KEY (user_id) REFERENCES app_user (id)
        );
        */
        // The schema.rs matches this.
        // BUT the original create.sql had:
        /*
        INSERT INTO approvals (
          request_id, realm, client_id, user_id, device_id, jkt, public_jwk,
          platform, model, app_version, reason, expires_at, context, idempotency_key
        )
        */
        // It seems the table name in create.sql was "approvals" (plural) while migration says "approval" (singular).
        // And schema.rs says "approval".
        // Also some columns are missing in migration/schema.rs: realm, client_id, device_id (it's new_device_id), jkt (it's new_device_jkt), reason, context, idempotency_key.
        
        // I must follow schema.rs and migration.
        
        let new_approval = db::ApprovalRow {
            request_id: request_id_val.clone(),
            user_id: req.user_id.clone(),
            new_device_id: req.new_device.device_id.clone(),
            new_device_jkt: req.new_device.jkt.clone(),
            new_device_public_jwk: public_jwk_str,
            new_device_platform: req.new_device.platform.clone(),
            new_device_model: req.new_device.model.clone(),
            new_device_app_version: req.new_device.app_version.clone(),
            status: "PENDING".to_string(),
            created_at: chrono::Utc::now(),
            expires_at: req.expires_at.unwrap_or_else(|| chrono::Utc::now() + chrono::Duration::minutes(15)),
            decided_at: None,
            decided_by_device_id: None,
            message: None,
        };

        diesel::insert_into(approval)
            .values(&new_approval)
            .execute(&mut conn)
            .await
            .map_err(|e| backend_core::Error::Diesel(e))?;

        Ok(ApprovalCreated {
            request_id: new_approval.request_id,
            status: new_approval.status,
            expires_at: Some(new_approval.expires_at),
        })
    }

    async fn get_approval(&self, request_id_val: &str) -> RepoResult<Option<db::ApprovalRow>> {
        use backend_model::schema::approval::dsl::*;

        let mut conn = self.get_conn().await?;

        approval
            .filter(request_id.eq(request_id_val))
            .first::<db::ApprovalRow>(&mut conn)
            .await
            .optional()
            .map_err(|e| backend_core::Error::Diesel(e))
    }

    async fn list_user_approvals(
        &self,
        user_id_val: &str,
        statuses: Option<Vec<String>>,
    ) -> RepoResult<Vec<db::ApprovalRow>> {
        use backend_model::schema::approval::dsl::*;

        let mut conn = self.get_conn().await?;
        let mut query = approval.into_boxed();

        query = query.filter(user_id.eq(user_id_val));

        if let Some(statuses_val) = statuses {
            query = query.filter(status.eq_any(statuses_val));
        }

        query
            .order(created_at.desc())
            .load::<db::ApprovalRow>(&mut conn)
            .await
            .map_err(|e| backend_core::Error::Diesel(e))
    }

    async fn decide_approval(
        &self,
        request_id_val: &str,
        req: &kc_map::ApprovalDecisionRequest,
    ) -> RepoResult<Option<db::ApprovalRow>> {
        use backend_model::schema::approval::dsl::*;

        let mut conn = self.get_conn().await?;

        let status_val = match req.decision.to_string().as_str() {
            "APPROVE" => "APPROVED".to_owned(),
            "DENY" => "DENIED".to_owned(),
            _ => "DENIED".to_owned(),
        };

        diesel::update(approval.filter(request_id.eq(request_id_val)))
            .set((
                status.eq(status_val),
                decided_at.eq(chrono::Utc::now()),
                decided_by_device_id.eq(req.decided_by_device_id.clone()),
                message.eq(req.message.clone()),
            ))
            .get_result::<db::ApprovalRow>(&mut conn)
            .await
            .optional()
            .map_err(|e| backend_core::Error::Diesel(e))
    }

    async fn cancel_approval(&self, request_id_val: &str) -> RepoResult<u64> {
        use backend_model::schema::approval::dsl::*;

        let mut conn = self.get_conn().await?;

        diesel::delete(approval.filter(request_id.eq(request_id_val)))
            .execute(&mut conn)
            .await
            .map(|n| n as u64)
            .map_err(|e| backend_core::Error::Diesel(e))
    }
}
