use crate::pg::{PgRepository, PgSqlRepo};
use crate::traits::*;
use backend_model::{db, kc as kc_map};

impl PgRepository {
    pub async fn create_approval(
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

    pub async fn get_approval(&self, request_id: &str) -> RepoResult<Option<db::ApprovalRow>> {
        let row = self.get_approval_db(request_id.to_owned()).await?;
        Ok(row)
    }

    pub async fn list_user_approvals(
        &self,
        user_id: &str,
        statuses: Option<Vec<String>>,
    ) -> RepoResult<Vec<db::ApprovalRow>> {
        let rows = self
            .list_user_approvals_db(user_id.to_owned(), statuses)
            .await?;
        Ok(rows)
    }

    pub async fn decide_approval(
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

    pub async fn cancel_approval(&self, request_id: &str) -> RepoResult<u64> {
        let res = self.cancel_approval_db(request_id.to_owned()).await?;
        Ok(res.rows_affected())
    }
}
