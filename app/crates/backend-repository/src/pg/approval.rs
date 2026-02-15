use crate::traits::*;
use backend_model::{db, kc as kc_map};
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx_data::{QueryResult, dml, repo};
use sqlx::PgPool;

#[repo]
pub trait PgApprovalRepo {
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
}

#[derive(Clone)]
pub struct ApprovalRepository {
    pub(crate) pool: PgPool,
}

impl PgApprovalRepo for ApprovalRepository {
    fn get_pool(&self) -> &sqlx_data::Pool {
        &self.pool
    }
}

impl ApprovalRepo for ApprovalRepository {
    async fn create_approval(
        &self,
        req: &kc_map::ApprovalCreateRequest,
        idempotency_key: Option<String>,
    ) -> RepoResult<ApprovalCreated> {
        let request_id = backend_id::approval_id()?;
        let public_jwk = req
            .new_device
            .public_jwk
            .clone()
            .map(|m| serde_json::to_value(m).unwrap_or_default());
        let ctx_json = req
            .context
            .clone()
            .map(|m| serde_json::to_value(m).unwrap_or_default());

        let (request_id, status, expires_at) = self
            .create_approval_db(
                request_id,
                req.realm.clone(),
                req.client_id.clone(),
                req.user_id.clone(),
                req.new_device.device_id.clone(),
                req.new_device.jkt.clone(),
                public_jwk,
                req.new_device.platform.clone(),
                req.new_device.model.clone(),
                req.new_device.app_version.clone(),
                req.reason.clone(),
                req.expires_at,
                ctx_json,
                idempotency_key,
            )
            .await?;

        Ok(ApprovalCreated {
            request_id,
            status,
            expires_at,
        })
    }

    async fn get_approval(&self, request_id: &str) -> RepoResult<Option<db::ApprovalRow>> {
        let row = self.get_approval_db(request_id.to_owned()).await?;
        Ok(row)
    }

    async fn list_user_approvals(
        &self,
        user_id: &str,
        statuses: Option<Vec<String>>,
    ) -> RepoResult<Vec<db::ApprovalRow>> {
        let rows = self
            .list_user_approvals_db(user_id.to_owned(), statuses)
            .await?;
        Ok(rows)
    }

    async fn decide_approval(
        &self,
        request_id: &str,
        req: &kc_map::ApprovalDecisionRequest,
    ) -> RepoResult<Option<db::ApprovalRow>> {
        let status = match req.decision.to_string().as_str() {
            "APPROVE" => "APPROVED".to_owned(),
            "DENY" => "DENIED".to_owned(),
            _ => "DENIED".to_owned(),
        };

        let row = self
            .decide_approval_db(
                request_id.to_owned(),
                status,
                req.decided_by_device_id.clone(),
                req.message.clone(),
            )
            .await?;
        Ok(row)
    }

    async fn cancel_approval(&self, request_id: &str) -> RepoResult<u64> {
        let res = self.cancel_approval_db(request_id.to_owned()).await?;
        Ok(res.rows_affected())
    }
}

impl ApprovalRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}
