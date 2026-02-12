use crate::context::{AuthContext, KcContext};
use crate::state::AppState;
use aws_sdk_s3::presigning::PresigningConfig;
use backend_model::db;
use backend_model::{bff as bff_map, kc as kc_map, staff as staff_map};
use chrono::{DateTime, Utc};
use gen_oas_server_bff::{
    Api as BffApi, ApiRegistrationKycDocumentsPostResponse, ApiRegistrationKycStatusGetResponse,
    ApiRegistrationLimitsGetResponse,
};
use gen_oas_server_kc::{
    Api as KcApi, CancelApprovalResponse, ConfirmSmsResponse, CreateApprovalResponse,
    CreateUserResponse, DecideApprovalResponse, DeleteUserResponse, EnrollmentBindResponse,
    EnrollmentPrecheckResponse, GetApprovalResponse, GetUserResponse, ListUserApprovalsResponse,
    ListUserDevicesResponse, LookupDeviceResponse, ResolveOrCreateUserByPhoneResponse,
    ResolveUserByPhoneResponse, SearchUsersResponse, SendSmsResponse, UpdateUserResponse,
};
use gen_oas_server_staff::{
    Api as StaffApi, ApiKycStaffSubmissionsExternalIdApprovePostResponse,
    ApiKycStaffSubmissionsExternalIdGetResponse, ApiKycStaffSubmissionsExternalIdRejectPostResponse,
    ApiKycStaffSubmissionsExternalIdRequestInfoPostResponse, ApiKycStaffSubmissionsGetResponse,
};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, QueryBuilder};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use swagger::ApiError;

#[derive(Clone)]
pub struct BackendApi {
    state: Arc<AppState>,
}

impl BackendApi {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }

    fn db(&self) -> &PgPool {
        &self.state.db
    }

    fn require_external_id(x_external_id: Option<String>) -> std::result::Result<String, ApiError> {
        x_external_id.ok_or_else(|| ApiError("Missing X-External-Id".to_owned()))
    }
}

fn kc_error(code: &str, message: &str) -> gen_oas_server_kc::models::Error {
    gen_oas_server_kc::models::Error::new(code.to_owned(), message.to_owned())
}

fn is_unique_violation(err: &sqlx::Error) -> bool {
    let sqlx::Error::Database(db_err) = err else {
        return false;
    };
    db_err.code().as_deref() == Some("23505")
}

// ---- BFF ----

#[backend_core::async_trait]
impl BffApi<AuthContext> for BackendApi {
    async fn api_registration_kyc_documents_post(
        &self,
        kyc_document_upload_request: gen_oas_server_bff::models::KycDocumentUploadRequest,
        x_external_id: Option<String>,
        _context: &AuthContext,
    ) -> std::result::Result<ApiRegistrationKycDocumentsPostResponse, ApiError> {
        let external_id = Self::require_external_id(x_external_id)?;
        let req: bff_map::KycDocumentUploadRequest = kyc_document_upload_request.into();

        // Upsert profile, then create document + presign.
        let now = Utc::now();
        let expires_at = now
            + chrono::Duration::seconds(self.state.config.aws.s3.presign_ttl_seconds as i64);
        let object_id = backend_core::cuid::cuid1().map_err(|e| ApiError(e.to_string()))?;
        let s3_key = format!("kyc/{external_id}/{object_id}/{}", req.file_name);

        let mut tx = self.db().begin().await.map_err(|e| ApiError(e.to_string()))?;

        sqlx::query(
            "INSERT INTO kyc_profiles (external_id) VALUES ($1) ON CONFLICT (external_id) DO NOTHING",
        )
        .bind(&external_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiError(e.to_string()))?;

        let doc_row: db::KycDocumentRow = sqlx::query_as(
            r#"
            INSERT INTO kyc_documents (
              external_id, document_type, file_name, mime_type, content_length,
              s3_bucket, s3_key, presigned_expires_at
            )
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
            RETURNING
              id::text as id,
              external_id,
              document_type,
              status::text as status,
              uploaded_at,
              rejection_reason,
              file_name,
              mime_type,
              content_length,
              s3_bucket,
              s3_key,
              presigned_expires_at,
              created_at,
              updated_at
            "#,
        )
        .bind(&external_id)
        .bind(&req.document_type)
        .bind(&req.file_name)
        .bind(&req.mime_type)
        .bind(req.content_length)
        .bind(&self.state.config.aws.s3.bucket)
        .bind(&s3_key)
        .bind(expires_at)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| ApiError(e.to_string()))?;

        tx.commit().await.map_err(|e| ApiError(e.to_string()))?;

        let presign_cfg = PresigningConfig::expires_in(Duration::from_secs(
            self.state.config.aws.s3.presign_ttl_seconds,
        ))
        .map_err(|e| ApiError(e.to_string()))?;

        let presigned = self
            .state
            .s3
            .put_object()
            .bucket(&self.state.config.aws.s3.bucket)
            .key(&s3_key)
            .content_type(req.mime_type.clone())
            .content_length(req.content_length)
            .presigned(presign_cfg)
            .await
            .map_err(|e| ApiError(e.to_string()))?;

        let headers = presigned
            .headers()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect::<HashMap<String, String>>();

        let dto = bff_map::KycDocumentUploadResponseDto {
            document_id: Some(doc_row.id),
            document_type: Some(doc_row.document_type),
            status: Some(doc_row.status),
            uploaded_at: doc_row.uploaded_at,
            file_name: Some(doc_row.file_name),
            mime_type: Some(doc_row.mime_type),
            upload_url: Some(presigned.uri().to_string()),
            upload_method: Some(presigned.method().to_string()),
            upload_headers: Some(headers),
            expires_at: Some(expires_at),
            s3_bucket: Some(doc_row.s3_bucket),
            s3_key: Some(doc_row.s3_key),
        };

        Ok(ApiRegistrationKycDocumentsPostResponse::UploadURLCreatedSuccessfully(dto.into()))
    }

    async fn api_registration_kyc_status_get(
        &self,
        x_external_id: Option<String>,
        _context: &AuthContext,
    ) -> std::result::Result<ApiRegistrationKycStatusGetResponse, ApiError> {
        let external_id = Self::require_external_id(x_external_id)?;

        let profile: Option<db::KycProfileRow> = sqlx::query_as(
            r#"
            SELECT
              external_id,
              first_name,
              last_name,
              email,
              phone_number,
              date_of_birth,
              nationality,
              kyc_tier,
              kyc_status::text as kyc_status,
              submitted_at,
              reviewed_at,
              reviewed_by,
              rejection_reason,
              review_notes,
              created_at,
              updated_at
            FROM kyc_profiles
            WHERE external_id = $1
            "#,
        )
        .bind(&external_id)
        .fetch_optional(self.db())
        .await
        .map_err(|e| ApiError(e.to_string()))?;

        let Some(profile) = profile else {
            return Ok(ApiRegistrationKycStatusGetResponse::CustomerNotFound);
        };

        let docs: Vec<db::KycDocumentRow> = sqlx::query_as(
            r#"
            SELECT
              id::text as id,
              external_id,
              document_type,
              status::text as status,
              uploaded_at,
              rejection_reason,
              file_name,
              mime_type,
              content_length,
              s3_bucket,
              s3_key,
              presigned_expires_at,
              created_at,
              updated_at
            FROM kyc_documents
            WHERE external_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(&external_id)
        .fetch_all(self.db())
        .await
        .map_err(|e| ApiError(e.to_string()))?;

        let documents = docs
            .into_iter()
            .map(bff_map::KycStatusDocumentStatusDto::from)
            .map(Into::into)
            .collect::<Vec<_>>();

        let dto = bff_map::KycStatusResponseDto {
            kyc_tier: Some(profile.kyc_tier),
            kyc_status: Some(profile.kyc_status),
            documents: Some(documents),
            required_documents: Some(vec![]),
            missing_documents: Some(vec![]),
        };

        Ok(ApiRegistrationKycStatusGetResponse::KYCStatusInformation(dto.into()))
    }

    async fn api_registration_limits_get(
        &self,
        x_external_id: Option<String>,
        _context: &AuthContext,
    ) -> std::result::Result<ApiRegistrationLimitsGetResponse, ApiError> {
        let external_id = Self::require_external_id(x_external_id)?;

        let profile: Option<(i32,)> =
            sqlx::query_as("SELECT kyc_tier FROM kyc_profiles WHERE external_id = $1")
                .bind(&external_id)
                .fetch_optional(self.db())
                .await
                .map_err(|e| ApiError(e.to_string()))?;

        let Some((kyc_tier,)) = profile else {
            return Ok(ApiRegistrationLimitsGetResponse::CustomerNotFound);
        };

        let mut resp = gen_oas_server_bff::models::LimitsResponse::new();
        resp.kyc_tier = Some(kyc_tier);
        resp.tier_name = Some(match kyc_tier {
            0 => "TIER_0",
            1 => "TIER_1",
            2 => "TIER_2",
            _ => "TIER_UNKNOWN",
        }
        .to_owned());
        resp.currency = Some("USD".to_owned());
        resp.allowed_payment_methods = Some(vec!["CARD".to_owned(), "BANK_TRANSFER".to_owned()]);
        resp.restricted_features = Some(vec![]);

        Ok(ApiRegistrationLimitsGetResponse::LimitsAndUsageDetails(resp))
    }
}

// ---- STAFF ----

#[backend_core::async_trait]
impl StaffApi<AuthContext> for BackendApi {
    async fn api_kyc_staff_submissions_get(
        &self,
        status: Option<String>,
        search: Option<String>,
        page: Option<i32>,
        limit: Option<i32>,
        _context: &AuthContext,
    ) -> std::result::Result<ApiKycStaffSubmissionsGetResponse, ApiError> {
        let page = page.unwrap_or(1).max(1);
        let limit = limit.unwrap_or(20).clamp(1, 100);
        let offset = (page - 1) * limit;

        let mut qb_count: QueryBuilder<Postgres> =
            QueryBuilder::new("SELECT COUNT(*)::int4 FROM kyc_profiles WHERE 1=1");
        if status.is_some() {
            qb_count.push(" AND kyc_status::text = ");
            qb_count.push_bind(status.clone().unwrap());
        }
        if let Some(s) = &search {
            qb_count.push(" AND (external_id ILIKE ");
            qb_count.push_bind(format!("%{s}%"));
            qb_count.push(" OR email ILIKE ");
            qb_count.push_bind(format!("%{s}%"));
            qb_count.push(" OR phone_number ILIKE ");
            qb_count.push_bind(format!("%{s}%"));
            qb_count.push(")");
        }
        let total: i32 = qb_count
            .build_query_scalar()
            .fetch_one(self.db())
            .await
            .map_err(|e| ApiError(e.to_string()))?;

        let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(
            r#"
            SELECT
              external_id,
              first_name,
              last_name,
              email,
              phone_number,
              date_of_birth,
              nationality,
              kyc_tier,
              kyc_status::text as kyc_status,
              submitted_at,
              reviewed_at,
              reviewed_by,
              rejection_reason,
              review_notes,
              created_at,
              updated_at
            FROM kyc_profiles
            WHERE 1=1
            "#,
        );
        if status.is_some() {
            qb.push(" AND kyc_status::text = ");
            qb.push_bind(status.unwrap());
        }
        if let Some(s) = &search {
            qb.push(" AND (external_id ILIKE ");
            qb.push_bind(format!("%{s}%"));
            qb.push(" OR email ILIKE ");
            qb.push_bind(format!("%{s}%"));
            qb.push(" OR phone_number ILIKE ");
            qb.push_bind(format!("%{s}%"));
            qb.push(")");
        }
        qb.push(" ORDER BY submitted_at DESC NULLS LAST, created_at DESC");
        qb.push(" LIMIT ");
        qb.push_bind(limit);
        qb.push(" OFFSET ");
        qb.push_bind(offset);

        let rows: Vec<db::KycProfileRow> = qb
            .build_query_as()
            .fetch_all(self.db())
            .await
            .map_err(|e| ApiError(e.to_string()))?;

        let items = rows
            .into_iter()
            .map(staff_map::KycSubmissionSummaryDto::from)
            .map(Into::into)
            .collect::<Vec<_>>();

        let dto = staff_map::KycSubmissionsResponseDto {
            items: Some(items),
            total: Some(total),
            page: Some(page),
            page_size: Some(limit),
        };

        Ok(ApiKycStaffSubmissionsGetResponse::PageOfKYCSubmissions(dto.into()))
    }

    async fn api_kyc_staff_submissions_external_id_get(
        &self,
        external_id: String,
        _context: &AuthContext,
    ) -> std::result::Result<ApiKycStaffSubmissionsExternalIdGetResponse, ApiError> {
        let profile: Option<db::KycProfileRow> = sqlx::query_as(
            r#"
            SELECT
              external_id,
              first_name,
              last_name,
              email,
              phone_number,
              date_of_birth,
              nationality,
              kyc_tier,
              kyc_status::text as kyc_status,
              submitted_at,
              reviewed_at,
              reviewed_by,
              rejection_reason,
              review_notes,
              created_at,
              updated_at
            FROM kyc_profiles
            WHERE external_id = $1
            "#,
        )
        .bind(&external_id)
        .fetch_optional(self.db())
        .await
        .map_err(|e| ApiError(e.to_string()))?;

        let Some(profile) = profile else {
            return Ok(ApiKycStaffSubmissionsExternalIdGetResponse::SubmissionNotFound);
        };

        let docs: Vec<db::KycDocumentRow> = sqlx::query_as(
            r#"
            SELECT
              id::text as id,
              external_id,
              document_type,
              status::text as status,
              uploaded_at,
              rejection_reason,
              file_name,
              mime_type,
              content_length,
              s3_bucket,
              s3_key,
              presigned_expires_at,
              created_at,
              updated_at
            FROM kyc_documents
            WHERE external_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(&external_id)
        .fetch_all(self.db())
        .await
        .map_err(|e| ApiError(e.to_string()))?;

        let mut dto = staff_map::KycSubmissionDetailResponseDto::from_profile(profile);
        let docs = docs
            .into_iter()
            .map(staff_map::KycDocumentDto::from)
            .map(Into::into)
            .collect::<Vec<_>>();
        dto.documents = Some(docs);

        Ok(ApiKycStaffSubmissionsExternalIdGetResponse::DetailedSubmission(dto.into()))
    }

    async fn api_kyc_staff_submissions_external_id_approve_post(
        &self,
        external_id: String,
        kyc_approval_request: gen_oas_server_staff::models::KycApprovalRequest,
        _context: &AuthContext,
    ) -> std::result::Result<ApiKycStaffSubmissionsExternalIdApprovePostResponse, ApiError> {
        let req: staff_map::KycApprovalRequest = kyc_approval_request.into();

        let updated = sqlx::query(
            r#"
            UPDATE kyc_profiles
            SET
              kyc_status = 'APPROVED',
              kyc_tier = $2,
              reviewed_at = now(),
              reviewed_by = 'staff',
              review_notes = $3,
              updated_at = now()
            WHERE external_id = $1
            "#,
        )
        .bind(&external_id)
        .bind(req.new_tier as i32)
        .bind(req.notes)
        .execute(self.db())
        .await
        .map_err(|e| ApiError(e.to_string()))?
        .rows_affected();

        if updated == 0 {
            return Ok(ApiKycStaffSubmissionsExternalIdApprovePostResponse::ValidationFailed);
        }

        Ok(ApiKycStaffSubmissionsExternalIdApprovePostResponse::KYCApproved)
    }

    async fn api_kyc_staff_submissions_external_id_reject_post(
        &self,
        external_id: String,
        kyc_rejection_request: gen_oas_server_staff::models::KycRejectionRequest,
        _context: &AuthContext,
    ) -> std::result::Result<ApiKycStaffSubmissionsExternalIdRejectPostResponse, ApiError> {
        let req: staff_map::KycRejectionRequest = kyc_rejection_request.into();

        let updated = sqlx::query(
            r#"
            UPDATE kyc_profiles
            SET
              kyc_status = 'REJECTED',
              reviewed_at = now(),
              reviewed_by = 'staff',
              rejection_reason = $2,
              review_notes = $3,
              updated_at = now()
            WHERE external_id = $1
            "#,
        )
        .bind(&external_id)
        .bind(req.reason)
        .bind(req.notes)
        .execute(self.db())
        .await
        .map_err(|e| ApiError(e.to_string()))?
        .rows_affected();

        if updated == 0 {
            return Ok(ApiKycStaffSubmissionsExternalIdRejectPostResponse::ValidationFailed);
        }

        Ok(ApiKycStaffSubmissionsExternalIdRejectPostResponse::KYCRejected)
    }

    async fn api_kyc_staff_submissions_external_id_request_info_post(
        &self,
        external_id: String,
        kyc_request_info_request: gen_oas_server_staff::models::KycRequestInfoRequest,
        _context: &AuthContext,
    ) -> std::result::Result<ApiKycStaffSubmissionsExternalIdRequestInfoPostResponse, ApiError> {
        let req: staff_map::KycRequestInfoRequest = kyc_request_info_request.into();

        let updated = sqlx::query(
            r#"
            UPDATE kyc_profiles
            SET
              kyc_status = 'NEEDS_INFO',
              reviewed_at = now(),
              reviewed_by = 'staff',
              review_notes = $2,
              updated_at = now()
            WHERE external_id = $1
            "#,
        )
        .bind(&external_id)
        .bind(req.message)
        .execute(self.db())
        .await
        .map_err(|e| ApiError(e.to_string()))?
        .rows_affected();

        if updated == 0 {
            return Ok(ApiKycStaffSubmissionsExternalIdRequestInfoPostResponse::ValidationFailed);
        }

        Ok(ApiKycStaffSubmissionsExternalIdRequestInfoPostResponse::AdditionalInfoRequested)
    }
}

// ---- KC ----

#[backend_core::async_trait]
impl KcApi<KcContext> for BackendApi {
    async fn create_user(
        &self,
        user_upsert_request: gen_oas_server_kc::models::UserUpsertRequest,
        _context: &KcContext,
    ) -> std::result::Result<CreateUserResponse, ApiError> {
        let req: kc_map::UserUpsert = user_upsert_request.into();
        let user_id = backend_core::cuid::cuid1().map_err(|e| ApiError(e.to_string()))?;

        let attributes_json = req.attributes.map(|m| serde_json::to_value(m).unwrap_or_default());

        let result = sqlx::query_as::<_, db::UserRow>(
            r#"
            INSERT INTO users (
              user_id, realm, username, first_name, last_name, email, enabled, email_verified, attributes
            )
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
            RETURNING
              user_id, realm, username, first_name, last_name, email, enabled, email_verified,
              attributes, created_at, updated_at
            "#,
        )
        .bind(&user_id)
        .bind(&req.realm)
        .bind(&req.username)
        .bind(req.first_name)
        .bind(req.last_name)
        .bind(req.email)
        .bind(req.enabled.unwrap_or(true))
        .bind(req.email_verified.unwrap_or(false))
        .bind(attributes_json)
        .fetch_one(self.db())
        .await;

        match result {
            Ok(row) => {
                let dto = kc_map::UserRecordDto::from(row);
                Ok(CreateUserResponse::Created(dto.into()))
            }
            Err(e) if is_unique_violation(&e) => Ok(CreateUserResponse::Conflict(kc_error(
                "CONFLICT",
                "User already exists",
            ))),
            Err(e) => Err(ApiError(e.to_string())),
        }
    }

    async fn get_user(
        &self,
        user_id: String,
        _context: &KcContext,
    ) -> std::result::Result<GetUserResponse, ApiError> {
        let row: Option<db::UserRow> = sqlx::query_as(
            r#"
            SELECT
              user_id, realm, username, first_name, last_name, email, enabled, email_verified,
              attributes, created_at, updated_at
            FROM users
            WHERE user_id = $1
            "#,
        )
        .bind(&user_id)
        .fetch_optional(self.db())
        .await
        .map_err(|e| ApiError(e.to_string()))?;

        let Some(row) = row else {
            return Ok(GetUserResponse::NotFound(kc_error("NOT_FOUND", "User not found")));
        };

        Ok(GetUserResponse::User(kc_map::UserRecordDto::from(row).into()))
    }

    async fn update_user(
        &self,
        user_id: String,
        user_upsert_request: gen_oas_server_kc::models::UserUpsertRequest,
        _context: &KcContext,
    ) -> std::result::Result<UpdateUserResponse, ApiError> {
        let req: kc_map::UserUpsert = user_upsert_request.into();
        let attributes_json = req.attributes.map(|m| serde_json::to_value(m).unwrap_or_default());

        let row: Option<db::UserRow> = sqlx::query_as(
            r#"
            UPDATE users
            SET
              realm = $2,
              username = $3,
              first_name = $4,
              last_name = $5,
              email = $6,
              enabled = $7,
              email_verified = $8,
              attributes = $9,
              updated_at = now()
            WHERE user_id = $1
            RETURNING
              user_id, realm, username, first_name, last_name, email, enabled, email_verified,
              attributes, created_at, updated_at
            "#,
        )
        .bind(&user_id)
        .bind(&req.realm)
        .bind(&req.username)
        .bind(req.first_name)
        .bind(req.last_name)
        .bind(req.email)
        .bind(req.enabled.unwrap_or(true))
        .bind(req.email_verified.unwrap_or(false))
        .bind(attributes_json)
        .fetch_optional(self.db())
        .await
        .map_err(|e| ApiError(e.to_string()))?;

        let Some(row) = row else {
            return Ok(UpdateUserResponse::NotFound(kc_error("NOT_FOUND", "User not found")));
        };

        Ok(UpdateUserResponse::Updated(kc_map::UserRecordDto::from(row).into()))
    }

    async fn delete_user(
        &self,
        user_id: String,
        _context: &KcContext,
    ) -> std::result::Result<DeleteUserResponse, ApiError> {
        let res = sqlx::query("DELETE FROM users WHERE user_id = $1")
            .bind(&user_id)
            .execute(self.db())
            .await
            .map_err(|e| ApiError(e.to_string()))?;

        if res.rows_affected() == 0 {
            return Ok(DeleteUserResponse::NotFound(kc_error("NOT_FOUND", "User not found")));
        }

        Ok(DeleteUserResponse::Deleted)
    }

    async fn search_users(
        &self,
        user_search_request: gen_oas_server_kc::models::UserSearchRequest,
        _context: &KcContext,
    ) -> std::result::Result<SearchUsersResponse, ApiError> {
        let req: kc_map::UserSearch = user_search_request.into();
        let max_results = req.max_results.unwrap_or(50).clamp(1, 200);
        let first_result = req.first_result.unwrap_or(0).max(0);

        let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(
            r#"
            SELECT
              user_id, realm, username, first_name, last_name, email, enabled, email_verified,
              attributes, created_at, updated_at
            FROM users
            WHERE realm = 
            "#,
        );
        qb.push_bind(&req.realm);

        if let Some(search) = &req.search {
            let like = format!("%{search}%");
            qb.push(" AND (username ILIKE ");
            qb.push_bind(like.clone());
            qb.push(" OR email ILIKE ");
            qb.push_bind(like.clone());
            qb.push(" OR first_name ILIKE ");
            qb.push_bind(like.clone());
            qb.push(" OR last_name ILIKE ");
            qb.push_bind(like);
            qb.push(")");
        }
        if let Some(username) = &req.username {
            qb.push(" AND username = ");
            qb.push_bind(username);
        }
        if let Some(email) = &req.email {
            qb.push(" AND email = ");
            qb.push_bind(email);
        }
        if let Some(enabled) = req.enabled {
            qb.push(" AND enabled = ");
            qb.push_bind(enabled);
        }
        if let Some(email_verified) = req.email_verified {
            qb.push(" AND email_verified = ");
            qb.push_bind(email_verified);
        }

        qb.push(" ORDER BY created_at DESC");
        qb.push(" LIMIT ");
        qb.push_bind(max_results);
        qb.push(" OFFSET ");
        qb.push_bind(first_result);

        let users: Vec<db::UserRow> = qb
            .build_query_as()
            .fetch_all(self.db())
            .await
            .map_err(|e| ApiError(e.to_string()))?;

        let out_users = users
            .into_iter()
            .map(kc_map::UserRecordDto::from)
            .map(Into::into)
            .collect::<Vec<_>>();

        let total_count = out_users.len() as i32;
        let resp = gen_oas_server_kc::models::UserSearchResponse {
            users: out_users,
            total_count: Some(total_count),
        };

        Ok(SearchUsersResponse::SearchResults(resp))
    }

    async fn lookup_device(
        &self,
        device_lookup_request: gen_oas_server_kc::models::DeviceLookupRequest,
        _context: &KcContext,
    ) -> std::result::Result<LookupDeviceResponse, ApiError> {
        let req: kc_map::DeviceLookupRequest = device_lookup_request.into();
        if req.device_id.is_none() && req.jkt.is_none() {
            return Ok(LookupDeviceResponse::BadRequest(kc_error(
                "BAD_REQUEST",
                "device_id or jkt must be set",
            )));
        }

        let row: Option<db::DeviceRow> = sqlx::query_as(
            r#"
            SELECT
              id::text as id,
              realm,
              client_id,
              user_id,
              user_hint,
              device_id,
              jkt,
              status::text as status,
              public_jwk,
              attributes,
              proof,
              label,
              created_at,
              last_seen_at
            FROM devices
            WHERE ($1::text IS NULL OR device_id = $1)
              AND ($2::text IS NULL OR jkt = $2)
            LIMIT 1
            "#,
        )
        .bind(req.device_id)
        .bind(req.jkt)
        .fetch_optional(self.db())
        .await
        .map_err(|e| ApiError(e.to_string()))?;

        let Some(row) = row else {
            return Ok(LookupDeviceResponse::NotFound(kc_error("NOT_FOUND", "Not found")));
        };

        let public_jwk = match &row.public_jwk {
            serde_json::Value::Object(map) => Some(map.clone().into_iter().collect()),
            _ => None,
        };

        let mut resp = gen_oas_server_kc::models::DeviceLookupResponse::new(true);
        resp.user_id = Some(row.user_id.clone());
        resp.device = Some(kc_map::DeviceRecordDto::from(row.clone()).into());
        resp.public_jwk = public_jwk;

        Ok(LookupDeviceResponse::LookupResult(resp))
    }

    async fn list_user_devices(
        &self,
        user_id: String,
        include_revoked: Option<bool>,
        _context: &KcContext,
    ) -> std::result::Result<ListUserDevicesResponse, ApiError> {
        let include_revoked = include_revoked.unwrap_or(false);

        let rows: Vec<db::DeviceRow> = if include_revoked {
            sqlx::query_as(
                r#"
                SELECT
                  id::text as id,
                  realm,
                  client_id,
                  user_id,
                  user_hint,
                  device_id,
                  jkt,
                  status::text as status,
                  public_jwk,
                  attributes,
                  proof,
                  label,
                  created_at,
                  last_seen_at
                FROM devices
                WHERE user_id = $1
                ORDER BY created_at DESC
                "#,
            )
            .bind(&user_id)
            .fetch_all(self.db())
            .await
        } else {
            sqlx::query_as(
                r#"
                SELECT
                  id::text as id,
                  realm,
                  client_id,
                  user_id,
                  user_hint,
                  device_id,
                  jkt,
                  status::text as status,
                  public_jwk,
                  attributes,
                  proof,
                  label,
                  created_at,
                  last_seen_at
                FROM devices
                WHERE user_id = $1
                  AND status = 'ACTIVE'
                ORDER BY created_at DESC
                "#,
            )
            .bind(&user_id)
            .fetch_all(self.db())
            .await
        }
        .map_err(|e| ApiError(e.to_string()))?;

        let devices = rows
            .into_iter()
            .map(kc_map::DeviceRecordDto::from)
            .map(Into::into)
            .collect::<Vec<_>>();

        Ok(ListUserDevicesResponse::DeviceList(
            gen_oas_server_kc::models::UserDevicesResponse { user_id, devices },
        ))
    }

    async fn disable_user_device(
        &self,
        user_id: String,
        device_id: String,
        _context: &KcContext,
    ) -> std::result::Result<gen_oas_server_kc::DisableUserDeviceResponse, ApiError> {
        let row: Option<db::DeviceRow> = sqlx::query_as(
            r#"
            SELECT
              id::text as id,
              realm,
              client_id,
              user_id,
              user_hint,
              device_id,
              jkt,
              status::text as status,
              public_jwk,
              attributes,
              proof,
              label,
              created_at,
              last_seen_at
            FROM devices
            WHERE user_id = $1 AND device_id = $2
            "#,
        )
        .bind(&user_id)
        .bind(&device_id)
        .fetch_optional(self.db())
        .await
        .map_err(|e| ApiError(e.to_string()))?;

        let Some(row) = row else {
            return Ok(gen_oas_server_kc::DisableUserDeviceResponse::NotFound(kc_error(
                "NOT_FOUND",
                "Device not found",
            )));
        };

        if row.status != "ACTIVE" {
            return Ok(gen_oas_server_kc::DisableUserDeviceResponse::DeviceCannotBeDisabledInItsCurrentState(
                kc_error("INVALID_STATE", "Device cannot be disabled"),
            ));
        }

        let updated: db::DeviceRow = sqlx::query_as(
            r#"
            UPDATE devices
            SET status = 'REVOKED'
            WHERE id = $1::uuid
            RETURNING
              id::text as id,
              realm,
              client_id,
              user_id,
              user_hint,
              device_id,
              jkt,
              status::text as status,
              public_jwk,
              attributes,
              proof,
              label,
              created_at,
              last_seen_at
            "#,
        )
        .bind(&row.id)
        .fetch_one(self.db())
        .await
        .map_err(|e| ApiError(e.to_string()))?;

        Ok(gen_oas_server_kc::DisableUserDeviceResponse::DeviceDisabled(
            kc_map::DeviceRecordDto::from(updated).into(),
        ))
    }

    async fn enrollment_precheck(
        &self,
        enrollment_precheck_request: gen_oas_server_kc::models::EnrollmentPrecheckRequest,
        _idempotency_key: Option<String>,
        _context: &KcContext,
    ) -> std::result::Result<EnrollmentPrecheckResponse, ApiError> {
        let req: kc_map::EnrollmentPrecheckRequest = enrollment_precheck_request.into();

        let existing: Option<(String,)> = sqlx::query_as(
            "SELECT user_id FROM devices WHERE device_id = $1 OR jkt = $2 LIMIT 1",
        )
        .bind(&req.device_id)
        .bind(&req.jkt)
        .fetch_optional(self.db())
        .await
        .map_err(|e| ApiError(e.to_string()))?;

        let mut resp = gen_oas_server_kc::models::EnrollmentPrecheckResponse::new(
            gen_oas_server_kc::models::EnrollmentPrecheckResponseDecision::Allow,
        );

        if let Some((bound_user_id,)) = existing {
            resp.decision = gen_oas_server_kc::models::EnrollmentPrecheckResponseDecision::Reject;
            resp.reason = Some("DEVICE_ALREADY_BOUND".to_owned());
            resp.bound_user_id = Some(bound_user_id);
        }

        Ok(EnrollmentPrecheckResponse::PolicyDecision(resp))
    }

    async fn enrollment_bind(
        &self,
        enrollment_bind_request: gen_oas_server_kc::models::EnrollmentBindRequest,
        _idempotency_key: Option<String>,
        _context: &KcContext,
    ) -> std::result::Result<EnrollmentBindResponse, ApiError> {
        let req: kc_map::EnrollmentBindRequest = enrollment_bind_request.into();

        let existing: Option<(String, String)> = sqlx::query_as(
            "SELECT id::text, user_id FROM devices WHERE device_id = $1 OR jkt = $2 LIMIT 1",
        )
        .bind(&req.device_id)
        .bind(&req.jkt)
        .fetch_optional(self.db())
        .await
        .map_err(|e| ApiError(e.to_string()))?;

        if let Some((device_record_id, bound_user_id)) = existing {
            if bound_user_id != req.user_id {
                return Ok(EnrollmentBindResponse::DeviceAlreadyBoundToADifferentUser(
                    kc_error("CONFLICT", "Device already bound"),
                ));
            }

            let mut resp = gen_oas_server_kc::models::EnrollmentBindResponse::new(
                gen_oas_server_kc::models::EnrollmentBindResponseStatus::AlreadyBound,
            );
            resp.device_record_id = Some(device_record_id);
            resp.bound_user_id = Some(bound_user_id);
            return Ok(EnrollmentBindResponse::Bound(resp));
        }

        let attributes_json = req
            .attributes
            .map(|m| serde_json::to_value(m).unwrap_or_default());
        let public_jwk = serde_json::to_value(req.public_jwk).unwrap_or_default();
        let proof = req.proof.map(|m| serde_json::to_value(m).unwrap_or_default());

        let inserted: (String,) = sqlx::query_as(
            r#"
            INSERT INTO devices (
              realm, client_id, user_id, user_hint, device_id, jkt, public_jwk, attributes, proof
            )
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
            RETURNING id::text
            "#,
        )
        .bind(&req.realm)
        .bind(&req.client_id)
        .bind(&req.user_id)
        .bind(req.user_hint)
        .bind(&req.device_id)
        .bind(&req.jkt)
        .bind(public_jwk)
        .bind(attributes_json)
        .bind(proof)
        .fetch_one(self.db())
        .await
        .map_err(|e| ApiError(e.to_string()))?;

        let mut resp = gen_oas_server_kc::models::EnrollmentBindResponse::new(
            gen_oas_server_kc::models::EnrollmentBindResponseStatus::Bound,
        );
        resp.device_record_id = Some(inserted.0);
        resp.bound_user_id = Some(req.user_id);

        Ok(EnrollmentBindResponse::Bound(resp))
    }

    async fn create_approval(
        &self,
        approval_create_request: gen_oas_server_kc::models::ApprovalCreateRequest,
        idempotency_key: Option<String>,
        _context: &KcContext,
    ) -> std::result::Result<CreateApprovalResponse, ApiError> {
        let req: kc_map::ApprovalCreateRequest = approval_create_request.into();

        let public_jwk = req
            .new_device
            .public_jwk
            .map(|m| serde_json::to_value(m).unwrap_or_default());
        let ctx_json = req.context.map(|m| serde_json::to_value(m).unwrap_or_default());

        let inserted: std::result::Result<(String, String, Option<DateTime<Utc>>), sqlx::Error> =
            sqlx::query_as(
                r#"
                INSERT INTO approvals (
                  realm, client_id, user_id, device_id, jkt, public_jwk,
                  platform, model, app_version, reason, expires_at, context, idempotency_key
                )
                VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
                RETURNING request_id::text, status::text, expires_at
                "#,
            )
            .bind(&req.realm)
            .bind(&req.client_id)
            .bind(&req.user_id)
            .bind(&req.new_device.device_id)
            .bind(&req.new_device.jkt)
            .bind(public_jwk)
            .bind(req.new_device.platform)
            .bind(req.new_device.model)
            .bind(req.new_device.app_version)
            .bind(req.reason)
            .bind(req.expires_at)
            .bind(ctx_json)
            .bind(idempotency_key)
            .fetch_one(self.db())
            .await;

        match inserted {
            Ok((request_id, status, expires_at)) => {
                let mut resp = gen_oas_server_kc::models::ApprovalCreateResponse::new(
                    request_id,
                    status.parse().unwrap_or(
                        gen_oas_server_kc::models::ApprovalCreateResponseStatus::Pending,
                    ),
                );
                resp.expires_at = expires_at;
                Ok(CreateApprovalResponse::Created(resp))
            }
            Err(e) if is_unique_violation(&e) => Ok(CreateApprovalResponse::Conflict(kc_error(
                "CONFLICT",
                "Duplicate idempotency key",
            ))),
            Err(e) => Err(ApiError(e.to_string())),
        }
    }

    async fn get_approval(
        &self,
        request_id: String,
        _context: &KcContext,
    ) -> std::result::Result<GetApprovalResponse, ApiError> {
        let row: Option<db::ApprovalRow> = sqlx::query_as(
            r#"
            SELECT
              request_id::text as request_id,
              realm,
              client_id,
              user_id,
              device_id,
              jkt,
              public_jwk,
              platform,
              model,
              app_version,
              reason,
              expires_at,
              context,
              idempotency_key,
              status::text as status,
              created_at,
              decided_at,
              decided_by_device_id,
              message
            FROM approvals
            WHERE request_id = $1::uuid
            "#,
        )
        .bind(&request_id)
        .fetch_optional(self.db())
        .await
        .map_err(|e| ApiError(e.to_string()))?;

        let Some(row) = row else {
            return Ok(GetApprovalResponse::NotFound(kc_error(
                "NOT_FOUND",
                "Approval not found",
            )));
        };

        Ok(GetApprovalResponse::Status(
            kc_map::ApprovalStatusDto::from(row).into(),
        ))
    }

    async fn list_user_approvals<'a>(
        &self,
        user_id: String,
        status: Option<&'a Vec<gen_oas_server_kc::models::ListUserApprovalsStatusParameterInner>>,
        _context: &KcContext,
    ) -> std::result::Result<ListUserApprovalsResponse, ApiError> {
        let statuses: Option<Vec<String>> = status.map(|v| v.iter().map(|s| s.to_string()).collect());

        let rows: Vec<db::ApprovalRow> = sqlx::query_as(
            r#"
            SELECT
              request_id::text as request_id,
              realm,
              client_id,
              user_id,
              device_id,
              jkt,
              public_jwk,
              platform,
              model,
              app_version,
              reason,
              expires_at,
              context,
              idempotency_key,
              status::text as status,
              created_at,
              decided_at,
              decided_by_device_id,
              message
            FROM approvals
            WHERE user_id = $1
              AND ($2::text[] IS NULL OR status::text = ANY($2))
            ORDER BY created_at DESC
            "#,
        )
        .bind(&user_id)
        .bind(statuses)
        .fetch_all(self.db())
        .await
        .map_err(|e| ApiError(e.to_string()))?;

        let approvals = rows
            .into_iter()
            .map(kc_map::UserApprovalRecordDto::from)
            .map(Into::into)
            .collect::<Vec<_>>();

        Ok(ListUserApprovalsResponse::ApprovalList(
            gen_oas_server_kc::models::UserApprovalsResponse { user_id, approvals },
        ))
    }

    async fn decide_approval(
        &self,
        request_id: String,
        approval_decision_request: gen_oas_server_kc::models::ApprovalDecisionRequest,
        _context: &KcContext,
    ) -> std::result::Result<DecideApprovalResponse, ApiError> {
        let req: kc_map::ApprovalDecisionRequest = approval_decision_request.into();

        let new_status = match req.decision {
            gen_oas_server_kc::models::ApprovalDecisionRequestDecision::Approve => "APPROVED",
            gen_oas_server_kc::models::ApprovalDecisionRequestDecision::Deny => "DENIED",
        };

        let row: Option<db::ApprovalRow> = sqlx::query_as(
            r#"
            UPDATE approvals
            SET
              status = $2::approval_status,
              decided_at = now(),
              decided_by_device_id = $3,
              message = $4
            WHERE request_id = $1::uuid
            RETURNING
              request_id::text as request_id,
              realm,
              client_id,
              user_id,
              device_id,
              jkt,
              public_jwk,
              platform,
              model,
              app_version,
              reason,
              expires_at,
              context,
              idempotency_key,
              status::text as status,
              created_at,
              decided_at,
              decided_by_device_id,
              message
            "#,
        )
        .bind(&request_id)
        .bind(new_status)
        .bind(req.decided_by_device_id)
        .bind(req.message)
        .fetch_optional(self.db())
        .await
        .map_err(|e| ApiError(e.to_string()))?;

        let Some(row) = row else {
            return Ok(DecideApprovalResponse::NotFound(kc_error(
                "NOT_FOUND",
                "Approval not found",
            )));
        };

        Ok(DecideApprovalResponse::UpdatedStatus(
            kc_map::ApprovalStatusDto::from(row).into(),
        ))
    }

    async fn cancel_approval(
        &self,
        request_id: String,
        _context: &KcContext,
    ) -> std::result::Result<CancelApprovalResponse, ApiError> {
        let res = sqlx::query("DELETE FROM approvals WHERE request_id = $1::uuid")
            .bind(&request_id)
            .execute(self.db())
            .await
            .map_err(|e| ApiError(e.to_string()))?;

        if res.rows_affected() == 0 {
            return Ok(CancelApprovalResponse::NotFound(kc_error(
                "NOT_FOUND",
                "Approval not found",
            )));
        }

        Ok(CancelApprovalResponse::Cancelled)
    }

    async fn resolve_user_by_phone(
        &self,
        phone_resolve_request: gen_oas_server_kc::models::PhoneResolveRequest,
        _context: &KcContext,
    ) -> std::result::Result<ResolveUserByPhoneResponse, ApiError> {
        let phone = phone_resolve_request.phone_number.clone();
        let realm = phone_resolve_request.realm.clone();

        let user: Option<db::UserRow> = sqlx::query_as(
            r#"
            SELECT
              user_id, realm, username, first_name, last_name, email, enabled, email_verified,
              attributes, created_at, updated_at
            FROM users
            WHERE realm = $1 AND username = $2
            "#,
        )
        .bind(&realm)
        .bind(&phone)
        .fetch_optional(self.db())
        .await
        .map_err(|e| ApiError(e.to_string()))?;

        let has_user = user.is_some();
        let has_device_credentials = if let Some(user) = &user {
            let (count,): (i64,) =
                sqlx::query_as("SELECT COUNT(*) FROM devices WHERE user_id = $1")
                    .bind(&user.user_id)
                    .fetch_one(self.db())
                    .await
                    .map_err(|e| ApiError(e.to_string()))?;
            count > 0
        } else {
            false
        };

        let mut resp =
            gen_oas_server_kc::models::PhoneResolveResponse::new(
                phone,
                has_user,
                has_device_credentials,
                gen_oas_server_kc::models::EnrollmentPath::Otp,
            );
        if let Some(user) = user {
            resp.user_id = Some(user.user_id);
            resp.username = Some(user.username);
        }
        Ok(ResolveUserByPhoneResponse::PhoneResolutionAndRoutingRecommendation(resp))
    }

    async fn resolve_or_create_user_by_phone(
        &self,
        phone_resolve_or_create_request: gen_oas_server_kc::models::PhoneResolveOrCreateRequest,
        _context: &KcContext,
    ) -> std::result::Result<ResolveOrCreateUserByPhoneResponse, ApiError> {
        let phone = phone_resolve_or_create_request.phone_number.clone();
        let realm = phone_resolve_or_create_request.realm.clone();

        let existing: Option<db::UserRow> = sqlx::query_as(
            r#"
            SELECT
              user_id, realm, username, first_name, last_name, email, enabled, email_verified,
              attributes, created_at, updated_at
            FROM users
            WHERE realm = $1 AND username = $2
            "#,
        )
        .bind(&realm)
        .bind(&phone)
        .fetch_optional(self.db())
        .await
        .map_err(|e| ApiError(e.to_string()))?;

        let (user, created) = if let Some(user) = existing {
            (user, false)
        } else {
            let user_id = backend_core::cuid::cuid1().map_err(|e| ApiError(e.to_string()))?;
            let mut attributes = HashMap::new();
            attributes.insert("phone_number".to_owned(), phone.clone());
            let attributes_json = serde_json::to_value(attributes).unwrap_or_default();

            let created_user = sqlx::query_as::<_, db::UserRow>(
                r#"
                INSERT INTO users (user_id, realm, username, enabled, email_verified, attributes)
                VALUES ($1,$2,$3,TRUE,FALSE,$4)
                RETURNING
                  user_id, realm, username, first_name, last_name, email, enabled, email_verified,
                  attributes, created_at, updated_at
                "#,
            )
            .bind(&user_id)
            .bind(&realm)
            .bind(&phone)
            .bind(attributes_json)
            .fetch_one(self.db())
            .await
            .map_err(|e| ApiError(e.to_string()))?;
            (created_user, true)
        };

        let resp = gen_oas_server_kc::models::PhoneResolveOrCreateResponse::new(
            phone,
            user.user_id,
            user.username,
            created,
        );
        Ok(ResolveOrCreateUserByPhoneResponse::UserResolvedOrCreated(resp))
    }

    async fn send_sms(
        &self,
        sms_send_request: gen_oas_server_kc::models::SmsSendRequest,
        _context: &KcContext,
    ) -> std::result::Result<SendSmsResponse, ApiError> {
        let req: kc_map::SmsSendRequest = sms_send_request.into();

        let hash = backend_core::cuid::cuid1().map_err(|e| ApiError(e.to_string()))?;
        let ttl_seconds: i32 = 300;

        let mut hasher = Sha256::new();
        hasher.update(req.otp.as_bytes());
        let otp_sha256 = hasher.finalize().to_vec();

        let message = format!("Your verification code is: {}", req.otp);
        let metadata = serde_json::json!({
            "message": message,
        });

        sqlx::query(
            r#"
            INSERT INTO sms_messages (
              realm, client_id, user_id, phone_number, hash, otp_sha256, ttl_seconds,
              max_attempts, next_retry_at, metadata
            )
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,now(),$9)
            "#,
        )
        .bind(&req.realm)
        .bind(&req.client_id)
        .bind(req.user_id)
        .bind(&req.phone_number)
        .bind(&hash)
        .bind(otp_sha256)
        .bind(ttl_seconds)
        .bind(self.state.config.aws.sns.max_attempts as i32)
        .bind(metadata)
        .execute(self.db())
        .await
        .map_err(|e| ApiError(e.to_string()))?;

        let mut resp = gen_oas_server_kc::models::SmsSendResponse::new(hash);
        resp.ttl_seconds = Some(ttl_seconds);
        resp.status = Some("PENDING".to_owned());
        Ok(SendSmsResponse::OTPQueued(resp))
    }

    async fn confirm_sms(
        &self,
        sms_confirm_request: gen_oas_server_kc::models::SmsConfirmRequest,
        _context: &KcContext,
    ) -> std::result::Result<ConfirmSmsResponse, ApiError> {
        let req: kc_map::SmsConfirmRequest = sms_confirm_request.into();

        let row: Option<db::SmsMessageRow> = sqlx::query_as(
            r#"
            SELECT
              id::text as id,
              realm,
              client_id,
              user_id,
              phone_number,
              hash,
              otp_sha256,
              ttl_seconds,
              status::text as status,
              attempt_count,
              max_attempts,
              next_retry_at,
              last_error,
              sns_message_id,
              session_id,
              trace_id,
              metadata,
              created_at,
              sent_at,
              confirmed_at
            FROM sms_messages
            WHERE hash = $1
            "#,
        )
        .bind(&req.hash)
        .fetch_optional(self.db())
        .await
        .map_err(|e| ApiError(e.to_string()))?;

        let Some(row) = row else {
            let mut resp = gen_oas_server_kc::models::SmsConfirmResponse::new(false);
            resp.reason = Some("NOT_FOUND".to_owned());
            return Ok(ConfirmSmsResponse::ConfirmationResult(resp));
        };

        if let Some(ttl) = row.ttl_seconds {
            let expires_at = row.created_at + chrono::Duration::seconds(ttl as i64);
            if Utc::now() > expires_at {
                let mut resp = gen_oas_server_kc::models::SmsConfirmResponse::new(false);
                resp.reason = Some("EXPIRED".to_owned());
                return Ok(ConfirmSmsResponse::ConfirmationResult(resp));
            }
        }

        let mut hasher = Sha256::new();
        hasher.update(req.otp.as_bytes());
        let provided = hasher.finalize().to_vec();

        if provided != row.otp_sha256 {
            let mut resp = gen_oas_server_kc::models::SmsConfirmResponse::new(false);
            resp.reason = Some("INVALID_OTP".to_owned());
            return Ok(ConfirmSmsResponse::ConfirmationResult(resp));
        }

        sqlx::query("UPDATE sms_messages SET confirmed_at = now() WHERE hash = $1")
            .bind(&req.hash)
            .execute(self.db())
            .await
            .map_err(|e| ApiError(e.to_string()))?;

        Ok(ConfirmSmsResponse::ConfirmationResult(
            gen_oas_server_kc::models::SmsConfirmResponse::new(true),
        ))
    }
}
