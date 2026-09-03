pub mod auth;
pub mod bff_flow;
pub mod bff_uploads;
mod date_deserialization_regression;
pub mod kc;
pub mod staff_flow;

use crate::state::AppState;
use axum::response::IntoResponse;
use backend_auth::{JwtToken, OidcState, SignatureContext, SignatureState};
use backend_core::{AppResult, Error};
use http::HeaderMap;
use std::sync::Arc;
use tracing::{debug, instrument};

pub(crate) const BFF_AUTH_USER_ID_HEADER: &str = "x-bff-authenticated-user-id";
pub(crate) const BFF_AUTH_DEVICE_ID_HEADER: &str = "x-bff-authenticated-device-id";
pub(crate) const BFF_AUTH_SUB_HEADER: &str = "x-bff-auth-sub";

#[derive(Debug, Clone)]
pub struct BffSignatureClaims {
    pub user_id: String,
    pub device_id: String,
    pub sub: Option<String>,
}

#[derive(Clone)]
pub struct BackendApi {
    pub(crate) state: Arc<AppState>,
    pub(crate) oidc_state: Arc<OidcState>,
    pub(crate) signature_state: Arc<SignatureState>,
}

impl AsRef<Self> for BackendApi {
    fn as_ref(&self) -> &Self {
        self
    }
}

impl BackendApi {
    pub fn new(
        state: Arc<AppState>,
        oidc_state: Arc<OidcState>,
        signature_state: Arc<SignatureState>,
    ) -> Self {
        Self {
            state,
            oidc_state,
            signature_state,
        }
    }

    pub(crate) fn require_bff_claims(&self, headers: &HeaderMap) -> AppResult<BffSignatureClaims> {
        if !self.state.config.bff.enabled {
            debug!("BFF auth disabled, returning mock claims");
            return Ok(BffSignatureClaims {
                user_id: "usr_auth_disabled".to_owned(),
                device_id: "dvc_auth_disabled".to_owned(),
                sub: None,
            });
        }

        Self::extract_bff_claims(headers).ok_or_else(|| {
            debug!("Missing BFF claims in headers");
            Error::unauthorized("Missing signature-authenticated BFF claims")
        })
    }

    pub(crate) fn extract_bff_claims(headers: &HeaderMap) -> Option<BffSignatureClaims> {
        let user_id = headers
            .get(BFF_AUTH_USER_ID_HEADER)
            .and_then(|value| value.to_str().ok())
            .map(ToOwned::to_owned)?;
        let device_id = headers
            .get(BFF_AUTH_DEVICE_ID_HEADER)
            .and_then(|value| value.to_str().ok())
            .map(ToOwned::to_owned)?;
        let sub = headers
            .get(BFF_AUTH_SUB_HEADER)
            .and_then(|value| value.to_str().ok())
            .map(ToOwned::to_owned);

        Some(BffSignatureClaims {
            user_id,
            device_id,
            sub,
        })
    }

    #[instrument(skip(context))]
    #[allow(dead_code)]
    pub(crate) fn require_user_id(context: &JwtToken) -> AppResult<String> {
        Ok(context.user_id().to_owned())
    }

    pub(crate) fn require_service_caller(
        &self,
        headers: &HeaderMap,
    ) -> AppResult<BffSignatureClaims> {
        let claims = self.require_bff_claims(headers)?;
        let sub = claims.sub.as_deref().ok_or_else(|| {
            debug!("Missing sub claim in BFF claims for service caller");
            Error::unauthorized("Service identity required: missing JWT sub claim")
        })?;

        let allowed = &self.state.config.bff.recovery_lookup_allowed_service_subs;
        if allowed.is_empty() {
            debug!("No allowed service subs configured, rejecting");
            return Err(Error::forbidden(
                "SERVICE_AUTH_DISABLED",
                "Recovery phone lookup service authentication is not configured",
            ));
        }

        if allowed.iter().any(|allowed_sub| allowed_sub == sub) {
            Ok(claims)
        } else {
            debug!(sub = %sub, "Service identity not in allowed list");
            Err(Error::forbidden(
                "SERVICE_IDENTITY_REJECTED",
                "Caller service identity is not authorized for phone lookup",
            ))
        }
    }
}

#[cfg(test)]
mod service_auth_tests {
    use super::*;
    use crate::test_utils::TestAppStateBuilder;

    fn api_with_allowed_service(allowed_subs: &[&str]) -> BackendApi {
        let mut state = TestAppStateBuilder::new().build();
        state.config.bff.recovery_lookup_allowed_service_subs = allowed_subs
            .iter()
            .map(|value| (*value).to_owned())
            .collect();
        let oidc_state = state.oidc_state.clone();
        let signature_state = state.signature_state.clone();
        BackendApi::new(Arc::new(state), oidc_state, signature_state)
    }

    fn authenticated_headers(subject: Option<&str>) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(BFF_AUTH_USER_ID_HEADER, "usr_caller".parse().unwrap());
        headers.insert(BFF_AUTH_DEVICE_ID_HEADER, "bff".parse().unwrap());
        if let Some(subject) = subject {
            headers.insert(BFF_AUTH_SUB_HEADER, subject.parse().unwrap());
        }
        headers
    }

    fn status(error: Error) -> u16 {
        match error {
            Error::Http { status_code, .. } => status_code,
            other => panic!("expected HTTP error, got {other:?}"),
        }
    }

    #[test]
    fn ordinary_user_jwt_is_forbidden() {
        let api = api_with_allowed_service(&["azamra-tokenization-bff"]);
        let error = api
            .require_service_caller(&authenticated_headers(Some("usr_customer")))
            .unwrap_err();
        assert_eq!(status(error), 403);
    }

    #[test]
    fn ordinary_device_authentication_is_unauthorized() {
        let api = api_with_allowed_service(&["azamra-tokenization-bff"]);
        let error = api
            .require_service_caller(&authenticated_headers(None))
            .unwrap_err();
        assert_eq!(status(error), 401);
    }

    #[test]
    fn unrelated_service_is_forbidden() {
        let api = api_with_allowed_service(&["azamra-tokenization-bff"]);
        let error = api
            .require_service_caller(&authenticated_headers(Some("reporting-service")))
            .unwrap_err();
        assert_eq!(status(error), 403);
    }

    #[test]
    fn recovery_bff_service_is_allowed() {
        let api = api_with_allowed_service(&["azamra-tokenization-bff"]);
        let claims = api
            .require_service_caller(&authenticated_headers(Some("azamra-tokenization-bff")))
            .unwrap();
        assert_eq!(claims.sub.as_deref(), Some("azamra-tokenization-bff"));
    }

    #[test]
    fn empty_allowlist_fails_closed() {
        let api = api_with_allowed_service(&[]);
        let error = api
            .require_service_caller(&authenticated_headers(Some("azamra-tokenization-bff")))
            .unwrap_err();
        assert_eq!(status(error), 403);
    }
}

pub(crate) fn kc_error(code: &str, message: &str) -> gen_oas_server_kc::models::Error {
    gen_oas_server_kc::models::Error::new(code.to_owned(), message.to_owned())
}

#[backend_core::async_trait]
impl gen_oas_server_kc::apis::ErrorHandler<Error> for BackendApi {
    #[instrument(skip(self, error))]
    async fn handle_error(
        &self,
        _method: &::http::Method,
        _host: &headers::Host,
        _cookies: &axum_extra::extract::CookieJar,
        error: Error,
    ) -> Result<axum::response::Response, http::StatusCode> {
        Ok(error.into_response())
    }
}

#[backend_core::async_trait]
impl gen_oas_server_kc::apis::ApiKeyAuthHeader for BackendApi {
    type Claims = SignatureContext;

    #[instrument(skip(self, _headers))]
    async fn extract_claims_from_header(
        &self,
        _headers: &HeaderMap,
        _key: &str,
    ) -> Option<Self::Claims> {
        Some(SignatureContext {})
    }
}
