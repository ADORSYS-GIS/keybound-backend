use super::BackendApi;
// LEGACY DISABLED: State machine endpoints retired
// These endpoints used the legacy sm_* tables which have been removed

use axum_extra::extract::CookieJar;
use backend_auth::JwtToken;
use backend_core::Error;
use gen_oas_server_staff::apis::kyc_state_machines::{
    KycStateMachines, StaffKycDepositsInstanceIdApprovePostResponse,
    StaffKycDepositsInstanceIdConfirmPaymentPostResponse, StaffKycInstancesGetResponse,
    StaffKycInstancesInstanceIdGetResponse, StaffKycInstancesInstanceIdRetryPostResponse,
    StaffKycReportsSummaryGetResponse,
};
use gen_oas_server_staff::models;
use headers::Host;
use http::Method;

#[backend_core::async_trait]
impl KycStateMachines<Error> for BackendApi {
    type Claims = JwtToken;

    async fn staff_kyc_instances_get(
        &self,
        _method: &Method,
        _host: &Host,
        _cookies: &CookieJar,
        _claims: &Self::Claims,
        _query_params: &models::StaffKycInstancesGetQueryParams,
    ) -> Result<StaffKycInstancesGetResponse, Error> {
        Err(Error::internal(
            "NOT_IMPLEMENTED",
            "Legacy state machine endpoints are retired",
        ))
    }

    async fn staff_kyc_instances_instance_id_get(
        &self,
        _method: &Method,
        _host: &Host,
        _cookies: &CookieJar,
        _claims: &Self::Claims,
        _path_params: &models::StaffKycInstancesInstanceIdGetPathParams,
    ) -> Result<StaffKycInstancesInstanceIdGetResponse, Error> {
        Err(Error::internal(
            "NOT_IMPLEMENTED",
            "Legacy state machine endpoints are retired",
        ))
    }

    async fn staff_kyc_instances_instance_id_retry_post(
        &self,
        _method: &Method,
        _host: &Host,
        _cookies: &CookieJar,
        _claims: &Self::Claims,
        _path_params: &models::StaffKycInstancesInstanceIdRetryPostPathParams,
        _body: &models::RetryRequest,
    ) -> Result<StaffKycInstancesInstanceIdRetryPostResponse, Error> {
        Err(Error::internal(
            "NOT_IMPLEMENTED",
            "Legacy state machine endpoints are retired",
        ))
    }

    async fn staff_kyc_deposits_instance_id_confirm_payment_post(
        &self,
        _method: &Method,
        _host: &Host,
        _cookies: &CookieJar,
        _claims: &Self::Claims,
        _path_params: &models::StaffKycDepositsInstanceIdConfirmPaymentPostPathParams,
        _body: &models::ConfirmPaymentRequest,
    ) -> Result<StaffKycDepositsInstanceIdConfirmPaymentPostResponse, Error> {
        Err(Error::internal(
            "NOT_IMPLEMENTED",
            "Legacy state machine endpoints are retired",
        ))
    }

    async fn staff_kyc_deposits_instance_id_approve_post(
        &self,
        _method: &Method,
        _host: &Host,
        _cookies: &CookieJar,
        _claims: &Self::Claims,
        _path_params: &models::StaffKycDepositsInstanceIdApprovePostPathParams,
        _body: &models::DepositApproveRequest,
    ) -> Result<StaffKycDepositsInstanceIdApprovePostResponse, Error> {
        Err(Error::internal(
            "NOT_IMPLEMENTED",
            "Legacy state machine endpoints are retired",
        ))
    }

    async fn staff_kyc_reports_summary_get(
        &self,
        _method: &Method,
        _host: &Host,
        _cookies: &CookieJar,
        _claims: &Self::Claims,
    ) -> Result<StaffKycReportsSummaryGetResponse, Error> {
        Err(Error::internal(
            "NOT_IMPLEMENTED",
            "Legacy state machine endpoints are retired",
        ))
    }
}
