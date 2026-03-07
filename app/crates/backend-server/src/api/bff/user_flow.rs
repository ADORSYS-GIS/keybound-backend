use super::super::BackendApi;
use super::shared::{
    KIND_KYC_FIRST_DEPOSIT, KIND_KYC_PHONE_OTP, ensure_user_match, parse_session_status,
    value_to_api_map,
};
use backend_auth::JwtToken;
use backend_core::Error;
use backend_model::db::{SmInstanceRow, UserRow};
use backend_repository::SmInstanceFilter;
use chrono::{DateTime, Utc};
use gen_oas_server_bff::apis::users::{
    InternalGetUserByIdResponse, InternalGetUserKycLevelResponse, InternalGetUserKycSummaryResponse,
};
use gen_oas_server_bff::models;

#[derive(Debug, Clone)]
struct UserKycProjection {
    level: models::UserKycLevel,
    phone_otp_verified: bool,
    first_deposit_verified: bool,
    phone_otp_status: Option<models::KycSessionStatus>,
    first_deposit_status: Option<models::KycSessionStatus>,
    latest_session_updated_at: Option<DateTime<Utc>>,
}

#[backend_core::async_trait]
pub(super) trait UserFlow {
    async fn get_user_by_id_flow(
        &self,
        claims: &JwtToken,
        path_params: &models::InternalGetUserByIdPathParams,
    ) -> Result<InternalGetUserByIdResponse, Error>;

    async fn get_user_kyc_level_flow(
        &self,
        claims: &JwtToken,
        path_params: &models::InternalGetUserKycLevelPathParams,
    ) -> Result<InternalGetUserKycLevelResponse, Error>;

    async fn get_user_kyc_summary_flow(
        &self,
        claims: &JwtToken,
        path_params: &models::InternalGetUserKycSummaryPathParams,
    ) -> Result<InternalGetUserKycSummaryResponse, Error>;
}

#[backend_core::async_trait]
impl UserFlow for BackendApi {
    async fn get_user_by_id_flow(
        &self,
        claims: &JwtToken,
        path_params: &models::InternalGetUserByIdPathParams,
    ) -> Result<InternalGetUserByIdResponse, Error> {
        ensure_user_match(claims, &path_params.user_id)?;
        let row = require_user(self, &path_params.user_id).await?;

        Ok(InternalGetUserByIdResponse::Status200_UserRow(
            user_record_from_row(row),
        ))
    }

    async fn get_user_kyc_level_flow(
        &self,
        claims: &JwtToken,
        path_params: &models::InternalGetUserKycLevelPathParams,
    ) -> Result<InternalGetUserKycLevelResponse, Error> {
        ensure_user_match(claims, &path_params.user_id)?;
        require_user(self, &path_params.user_id).await?;

        let projection = build_user_kyc_projection(self, &path_params.user_id).await?;
        let payload = models::UserKycLevelResponse::new(
            path_params.user_id.clone(),
            projection.level,
            projection.phone_otp_verified,
            projection.first_deposit_verified,
        );

        Ok(InternalGetUserKycLevelResponse::Status200_KYCLevel(payload))
    }

    async fn get_user_kyc_summary_flow(
        &self,
        claims: &JwtToken,
        path_params: &models::InternalGetUserKycSummaryPathParams,
    ) -> Result<InternalGetUserKycSummaryResponse, Error> {
        ensure_user_match(claims, &path_params.user_id)?;
        require_user(self, &path_params.user_id).await?;

        let projection = build_user_kyc_projection(self, &path_params.user_id).await?;
        let mut payload =
            models::UserKycSummary::new(path_params.user_id.clone(), projection.level);
        payload.phone_otp_status = projection.phone_otp_status;
        payload.first_deposit_status = projection.first_deposit_status;
        payload.latest_session_updated_at = projection.latest_session_updated_at;

        Ok(InternalGetUserKycSummaryResponse::Status200_KYCSummary(
            payload,
        ))
    }
}

async fn require_user(api: &BackendApi, user_id: &str) -> Result<UserRow, Error> {
    api.state
        .user
        .get_user(user_id)
        .await?
        .ok_or_else(|| Error::not_found("USER_NOT_FOUND", "User not found"))
}

fn user_record_from_row(row: UserRow) -> models::InternalUserRecord {
    models::InternalUserRecord {
        user_id: row.user_id,
        realm: row.realm,
        username: row.username,
        full_name: row.full_name,
        email: row.email,
        email_verified: row.email_verified,
        phone_number: row.phone_number,
        fineract_customer_id: row.fineract_customer_id,
        disabled: row.disabled,
        attributes: row.attributes.and_then(|value| value_to_api_map(&value)),
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

async fn build_user_kyc_projection(
    api: &BackendApi,
    user_id: &str,
) -> Result<UserKycProjection, Error> {
    let phone_otp_instance = latest_instance_for_kind(api, user_id, KIND_KYC_PHONE_OTP).await?;
    let first_deposit_instance =
        latest_instance_for_kind(api, user_id, KIND_KYC_FIRST_DEPOSIT).await?;

    let phone_otp_status = phone_otp_instance
        .as_ref()
        .map(|instance| parse_session_status(&instance.status, &instance.context));
    let first_deposit_status = first_deposit_instance
        .as_ref()
        .map(|instance| parse_session_status(&instance.status, &instance.context));

    let phone_otp_verified = matches!(phone_otp_status, Some(models::KycSessionStatus::Completed));
    let first_deposit_verified = matches!(
        first_deposit_status,
        Some(models::KycSessionStatus::Completed)
    );

    let level = if first_deposit_verified {
        models::UserKycLevel::FirstDepositVerified
    } else if phone_otp_verified {
        models::UserKycLevel::PhoneOtpVerified
    } else {
        models::UserKycLevel::None
    };

    let latest_session_updated_at = [phone_otp_instance, first_deposit_instance]
        .into_iter()
        .flatten()
        .map(|instance| instance.updated_at)
        .max();

    Ok(UserKycProjection {
        level,
        phone_otp_verified,
        first_deposit_verified,
        phone_otp_status,
        first_deposit_status,
        latest_session_updated_at,
    })
}

async fn latest_instance_for_kind(
    api: &BackendApi,
    user_id: &str,
    kind: &str,
) -> Result<Option<SmInstanceRow>, Error> {
    let (instances, _) = api
        .state
        .sm
        .list_instances(SmInstanceFilter {
            kind: Some(kind.to_owned()),
            status: None,
            user_id: Some(user_id.to_owned()),
            phone_number: None,
            created_from: None,
            created_to: None,
            page: 1,
            limit: 1,
        })
        .await?;

    Ok(instances.into_iter().next())
}
