use crate::pg::{PgRepository, PgSqlRepo};
use crate::traits::*;
use backend_model::{db, staff as staff_map};
use sqlx_data::{IntoParams, Serial};

impl PgRepository {
    pub async fn ensure_kyc_profile(&self, external_id: &str) -> RepoResult<()> {
        self.ensure_kyc_profile_db(external_id.to_owned()).await?;
        Ok(())
    }

    pub async fn insert_kyc_document_intent(
        &self,
        input: KycDocumentInsert,
    ) -> RepoResult<db::KycDocumentRow> {
        let id = backend_id::kyc_document_id()?;

        let row = self
            .insert_kyc_document_intent_db(
                id,
                input.external_id,
                input.document_type,
                input.file_name,
                input.mime_type,
                input.content_length,
                input.s3_bucket,
                input.s3_key,
                input.presigned_expires_at,
            )
            .await?;
        Ok(row)
    }

    pub async fn get_kyc_profile(
        &self,
        external_id: &str,
    ) -> RepoResult<Option<db::KycProfileRow>> {
        let row = self.get_kyc_profile_db(external_id.to_owned()).await?;
        Ok(row)
    }

    pub async fn list_kyc_documents(
        &self,
        external_id: String,
        params: impl IntoParams + Send,
    ) -> RepoResult<Serial<db::KycDocumentRow>> {
        let rows = self.list_kyc_documents_db(external_id, params).await?;
        Ok(rows)
    }

    pub async fn get_kyc_tier(&self, external_id: &str) -> RepoResult<Option<i32>> {
        let tier = self.get_kyc_tier_db(external_id.to_owned()).await?;
        Ok(tier)
    }

    pub async fn list_kyc_submissions(
        &self,
        params: impl IntoParams + Send,
    ) -> RepoResult<Serial<db::KycProfileRow>> {
        let rows = self.list_kyc_submissions_db(params).await?;
        Ok(rows)
    }

    pub async fn get_kyc_submission(
        &self,
        external_id: &str,
    ) -> RepoResult<Option<db::KycProfileRow>> {
        let row = self.get_kyc_profile_db(external_id.to_owned()).await?;
        Ok(row)
    }

    pub async fn update_kyc_approved(
        &self,
        external_id: &str,
        req: &staff_map::KycApprovalRequest,
    ) -> RepoResult<bool> {
        let res = self
            .update_kyc_approved_db(
                external_id.to_owned(),
                req.new_tier as i32,
                req.notes.clone(),
            )
            .await?;
        Ok(res.rows_affected() > 0)
    }

    pub async fn update_kyc_rejected(
        &self,
        external_id: &str,
        req: &staff_map::KycRejectionRequest,
    ) -> RepoResult<bool> {
        let res = self
            .update_kyc_rejected_db(
                external_id.to_owned(),
                req.reason.clone(),
                req.notes.clone(),
            )
            .await?;
        Ok(res.rows_affected() > 0)
    }

    pub async fn update_kyc_request_info(
        &self,
        external_id: &str,
        req: &staff_map::KycRequestInfoRequest,
    ) -> RepoResult<bool> {
        let res = self
            .update_kyc_request_info_db(external_id.to_owned(), req.message.clone())
            .await?;
        Ok(res.rows_affected() > 0)
    }

    pub async fn patch_kyc_information(
        &self,
        external_id: &str,
        req: &backend_model::bff::KycInformationPatchRequest,
    ) -> RepoResult<Option<db::KycProfileRow>> {
        let row = self
            .patch_kyc_information_db(
                external_id.to_owned(),
                req.first_name.clone(),
                req.last_name.clone(),
                req.email.clone(),
                req.phone_number.clone(),
                req.date_of_birth.clone(),
                req.nationality.clone(),
            )
            .await?;
        Ok(row)
    }
}
