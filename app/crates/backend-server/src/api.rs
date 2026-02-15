use crate::state::AppState;
use backend_auth::ServiceContext;
use backend_core::Error;
use gen_oas_server_bff::apis::ErrorHandler;
use sha2::Digest;
use std::sync::Arc;
use swagger::ApiError;

#[derive(Clone)]
pub struct BackendApi {
    state: Arc<AppState>,
}

impl BackendApi {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }

    fn require_user_id(context: &ServiceContext) -> std::result::Result<String, ApiError> {
        context
            .user_id()
            .map(ToOwned::to_owned)
            .ok_or_else(|| ApiError("Missing bearer subject".to_owned()))
    }

    fn normalize_page_limit(page: Option<i32>, limit: Option<i32>) -> (i32, i32) {
        let page = page.unwrap_or(1).max(1);
        let limit = limit.unwrap_or(20).clamp(1, 100);
        (page, limit)
    }
}

fn kc_error(code: &str, message: &str) -> gen_oas_server_kc::models::Error {
    gen_oas_server_kc::models::Error::new(code.to_owned(), message.to_owned())
}

fn repo_err(err: Error) -> ApiError {
    ApiError(err.to_string())
}

fn is_unique_violation(err: &Error) -> bool {
    matches!(
        err,
        Error::SqlxError(sqlx::Error::Database(db_err)) if db_err.code().as_deref() == Some("23505")
    )
}

impl ErrorHandler<()> for BackendApi {}

/// ## BFF APIs
///
#[backend_core::async_trait]
impl gen_oas_server_bff::apis::kyc::Kyc for BackendApi {}

#[backend_core::async_trait]
impl gen_oas_server_bff::apis::kyc_documents::KycDocuments for BackendApi {}

/// ## KC APIs
///
#[backend_core::async_trait]
impl gen_oas_server_kc::apis::approvals::Approvals for BackendApi {}

#[backend_core::async_trait]
impl gen_oas_server_kc::apis::devices::Devices for BackendApi {}

#[backend_core::async_trait]
impl gen_oas_server_kc::apis::users::Users for BackendApi {}

#[backend_core::async_trait]
impl gen_oas_server_kc::apis::enrollment::Enrollment for BackendApi {}

/// ## Staff APIs
///
#[backend_core::async_trait]
impl gen_oas_server_staff::apis::kyc_review::KycReview for BackendApi {}
