use super::{BackendApi, kc_error};
use axum_extra::extract::CookieJar;
use backend_auth::SignatureContext;
use backend_core::Error;
use backend_model::kc::{DeviceRecordDto, UserRecordDto, UserSearch, UserUpsert};
use gen_oas_server_kc::apis::devices::{Devices, LookupDeviceResponse};
use gen_oas_server_kc::apis::enrollment::{Enrollment, EnrollmentBindResponse};
use gen_oas_server_kc::apis::recovery::{GetRecoveryCaseResponse, Recovery, RecoveryBindResponse};
use gen_oas_server_kc::apis::users::{
    CreateUserResponse, DeleteUserResponse, GetUserResponse, SearchUsersResponse,
    UpdateUserResponse, Users,
};
use gen_oas_server_kc::models;
use headers::Host;
use http::Method;
use tracing::info;

#[backend_core::async_trait]
impl Devices<Error> for BackendApi {
    type Claims = SignatureContext;

    async fn lookup_device(
        &self,
        _method: &Method,
        _host: &Host,
        _cookies: &CookieJar,
        _claims: &Self::Claims,
        body: &models::DeviceLookupRequest,
    ) -> Result<LookupDeviceResponse, Error> {
        let req = backend_model::kc::DeviceLookupRequest {
            device_id: body.device_id.clone(),
            jkt: body.jkt.clone(),
        };

        self.state
            .device
            .lookup_device(&req)
            .await
            .map(|res| match res {
                Some(row) => {
                    let user_id = row.user_id.clone();
                    let public_jwk: Option<
                        std::collections::HashMap<String, gen_oas_server_kc::types::Object>,
                    > = serde_json::from_str(&row.public_jwk).ok();
                    let dto = DeviceRecordDto::from(row);
                    LookupDeviceResponse::Status200_LookupResult(models::DeviceLookupResponse {
                        device: Some(dto.into()),
                        found: true,
                        public_jwk,
                        user_id: Some(user_id),
                    })
                }
                None => LookupDeviceResponse::Status404_NotFound(kc_error(
                    "NOT_FOUND",
                    "Device not found",
                )),
            })
    }
}

#[backend_core::async_trait]
impl Users<Error> for BackendApi {
    type Claims = SignatureContext;

    async fn create_user(
        &self,
        _method: &Method,
        _host: &Host,
        _cookies: &CookieJar,
        _claims: &Self::Claims,
        body: &models::UserUpsertRequest,
    ) -> Result<CreateUserResponse, Error> {
        let req = UserUpsert::from(body.clone());
        let row = self.state.user.create_user(&req).await?;
        let user_data = self.state.user.list_user_data(&row.user_id, true).await?;
        let dto = UserRecordDto::from_row_with_user_data(row, &user_data);
        Ok(CreateUserResponse::Status201_Created(dto.into()))
    }

    async fn delete_user(
        &self,
        _method: &Method,
        _host: &Host,
        _cookies: &CookieJar,
        _claims: &Self::Claims,
        path_params: &models::DeleteUserPathParams,
    ) -> Result<DeleteUserResponse, Error> {
        self.state
            .user
            .delete_user(&path_params.user_id)
            .await
            .map(|count| {
                if count > 0 {
                    DeleteUserResponse::Status204_Deleted
                } else {
                    DeleteUserResponse::Status404_NotFound(kc_error("NOT_FOUND", "User not found"))
                }
            })
    }

    async fn get_user(
        &self,
        _method: &Method,
        _host: &Host,
        _cookies: &CookieJar,
        _claims: &Self::Claims,
        path_params: &models::GetUserPathParams,
    ) -> Result<GetUserResponse, Error> {
        let user = self.state.user.get_user(&path_params.user_id).await?;
        match user {
            Some(row) => {
                let user_data = self.state.user.list_user_data(&row.user_id, true).await?;
                info!("row >> {:?}", row);
                info!("user_data >> {:?}", user_data);
                let dto = UserRecordDto::from_row_with_user_data(row, &user_data);
                Ok(GetUserResponse::Status200_User(dto.into()))
            }
            None => Ok(GetUserResponse::Status404_NotFound(kc_error(
                "NOT_FOUND",
                "User not found",
            ))),
        }
    }

    async fn search_users(
        &self,
        _method: &Method,
        _host: &Host,
        _cookies: &CookieJar,
        _claims: &Self::Claims,
        body: &models::UserSearchRequest,
    ) -> Result<SearchUsersResponse, Error> {
        let req = UserSearch::from(body.clone());
        let rows = self.state.user.search_users(&req).await?;
        let mut users = Vec::with_capacity(rows.len());
        for row in rows {
            let user_data = self.state.user.list_user_data(&row.user_id, true).await?;
            users.push(UserRecordDto::from_row_with_user_data(row, &user_data).into());
        }

        Ok(SearchUsersResponse::Status200_SearchResults(
            models::UserSearchResponse {
                users,
                total_count: None,
            },
        ))
    }

    async fn update_user(
        &self,
        _method: &Method,
        _host: &Host,
        _cookies: &CookieJar,
        _claims: &Self::Claims,
        path_params: &models::UpdateUserPathParams,
        body: &models::UserUpsertRequest,
    ) -> Result<UpdateUserResponse, Error> {
        let req = UserUpsert::from(body.clone());
        let user = self
            .state
            .user
            .update_user(&path_params.user_id, &req)
            .await?;
        match user {
            Some(row) => {
                let user_data = self.state.user.list_user_data(&row.user_id, true).await?;
                let dto = UserRecordDto::from_row_with_user_data(row, &user_data);
                Ok(UpdateUserResponse::Status200_Updated(dto.into()))
            }
            None => Ok(UpdateUserResponse::Status404_NotFound(kc_error(
                "NOT_FOUND",
                "User not found",
            ))),
        }
    }
}

#[backend_core::async_trait]
impl Enrollment<Error> for BackendApi {
    type Claims = SignatureContext;

    async fn enrollment_bind(
        &self,
        _method: &Method,
        _host: &Host,
        _cookies: &CookieJar,
        _claims: &Self::Claims,
        _header_params: &models::EnrollmentBindHeaderParams,
        body: &models::EnrollmentBindRequest,
    ) -> Result<EnrollmentBindResponse, Error> {
        let req = backend_model::kc::EnrollmentBindRequest::from(body.clone());

        // Check if device is already bound to someone else
        let existing = self
            .state
            .device
            .find_device_binding(&req.device_id, &req.jkt)
            .await?;

        if let Some((bound_user_id, _)) = existing
            && bound_user_id != req.user_id
        {
            return Ok(
                EnrollmentBindResponse::Status409_DeviceAlreadyBoundToADifferentUser(kc_error(
                    "CONFLICT",
                    "Device already bound to another user",
                )),
            );
        }

        self.state.device.bind_device(&req).await.map(|record_id| {
            EnrollmentBindResponse::Status200_Bound(models::EnrollmentBindResponse {
                status: models::EnrollmentBindResponseStatus::Bound,
                device_record_id: Some(record_id),
                bound_user_id: req.user_id,
            })
        })
    }
}

fn compute_recovery_bind_hash(body: &models::RecoveryBindRequest) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(body.realm.as_bytes());
    hasher.update(b"|");
    hasher.update(body.target_user_id.as_bytes());
    hasher.update(b"|");
    hasher.update(body.approval_revision.to_string().as_bytes());
    hasher.update(b"|");
    hasher.update(body.device_id.as_bytes());
    hasher.update(b"|");
    hasher.update(body.jkt.as_bytes());
    hasher.update(b"|");
    hasher.update(body.binding_operation_id.as_bytes());
    hasher.update(b"|");

    let mut sorted_jwk: std::collections::BTreeMap<String, serde_json::Value> =
        std::collections::BTreeMap::new();
    for (k, v) in &body.public_jwk {
        sorted_jwk.insert(k.clone(), v.0.clone());
    }
    if let Ok(jwk_json) = serde_json::to_string(&sorted_jwk) {
        hasher.update(jwk_json.as_bytes());
    }
    hex::encode(hasher.finalize())
}

#[backend_core::async_trait]
impl Recovery<Error> for BackendApi {
    type Claims = SignatureContext;

    async fn get_recovery_case(
        &self,
        _method: &Method,
        _host: &Host,
        _cookies: &CookieJar,
        _claims: &Self::Claims,
        _path_params: &models::GetRecoveryCasePathParams,
    ) -> Result<GetRecoveryCaseResponse, Error> {
        Ok(GetRecoveryCaseResponse::Status404_NotFound(kc_error(
            "NOT_FOUND",
            "Recovery case not found",
        )))
    }

    async fn recovery_bind(
        &self,
        _method: &Method,
        _host: &Host,
        _cookies: &CookieJar,
        _claims: &Self::Claims,
        header_params: &models::RecoveryBindHeaderParams,
        path_params: &models::RecoveryBindPathParams,
        body: &models::RecoveryBindRequest,
    ) -> Result<RecoveryBindResponse, Error> {
        // 1. Validate Idempotency-Key header is a valid UUID
        if uuid::Uuid::parse_str(&header_params.idempotency_key).is_err() {
            return Ok(RecoveryBindResponse::Status400_BadRequest(kc_error(
                "BAD_REQUEST",
                "Idempotency-Key header must be a valid UUID string",
            )));
        }

        // 2. Validate target_user_id presence
        if body.target_user_id.trim().is_empty() {
            return Ok(RecoveryBindResponse::Status400_BadRequest(kc_error(
                "BAD_REQUEST",
                "target_user_id is required",
            )));
        }

        let req_hash = compute_recovery_bind_hash(body);

        // 3. Check for existing idempotency record
        let existing_idempotency = self
            .state
            .device
            .find_recovery_idempotency(&header_params.idempotency_key)
            .await?;

        if let Some(existing) = existing_idempotency {
            if existing.request_hash == req_hash {
                return Ok(RecoveryBindResponse::Status200_Bound(
                    models::EnrollmentBindResponse {
                        status: models::EnrollmentBindResponseStatus::AlreadyBound,
                        device_record_id: None,
                        bound_user_id: existing.bound_user_id,
                    },
                ));
            } else {
                return Ok(RecoveryBindResponse::Status409_Conflict(kc_error(
                    "CONFLICT",
                    "Idempotency-Key reused with modified payload",
                )));
            }
        }

        // 4. Map request and execute binding
        let domain_req = backend_model::kc::RecoveryBindRequest::from(body.clone());
        let bind_res = self
            .state
            .device
            .bind_recovery_device(
                &header_params.idempotency_key,
                &path_params.recovery_case_id,
                &req_hash,
                &domain_req,
            )
            .await;

        match bind_res {
            Ok(record_id) => Ok(RecoveryBindResponse::Status200_Bound(
                models::EnrollmentBindResponse {
                    status: models::EnrollmentBindResponseStatus::Bound,
                    device_record_id: Some(record_id),
                    bound_user_id: body.target_user_id.clone(),
                },
            )),
            Err(Error::Http {
                status_code: 409,
                error_key,
                message,
                ..
            }) => Ok(RecoveryBindResponse::Status409_Conflict(kc_error(
                error_key, &message,
            ))),
            Err(Error::Http {
                status_code: 400,
                error_key,
                message,
                ..
            }) => Ok(RecoveryBindResponse::Status400_BadRequest(kc_error(
                error_key, &message,
            ))),
            Err(err) => Err(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_recovery_bind_hash_determinism() {
        let mut public_jwk = std::collections::HashMap::new();
        public_jwk.insert(
            "kty".to_string(),
            gen_oas_server_kc::types::Object(serde_json::json!("EC")),
        );
        public_jwk.insert(
            "crv".to_string(),
            gen_oas_server_kc::types::Object(serde_json::json!("P-256")),
        );

        let req1 = models::RecoveryBindRequest {
            realm: "azamra".to_string(),
            target_user_id: "usr_123".to_string(),
            approval_revision: 1,
            device_id: "dvc_456".to_string(),
            jkt: "jkt_789".to_string(),
            public_jwk: public_jwk.clone(),
            binding_operation_id: "op_001".to_string(),
        };

        let req2 = models::RecoveryBindRequest {
            realm: "azamra".to_string(),
            target_user_id: "usr_123".to_string(),
            approval_revision: 1,
            device_id: "dvc_456".to_string(),
            jkt: "jkt_789".to_string(),
            public_jwk: public_jwk.clone(),
            binding_operation_id: "op_001".to_string(),
        };

        let hash1 = compute_recovery_bind_hash(&req1);
        let hash2 = compute_recovery_bind_hash(&req2);
        assert_eq!(hash1, hash2);

        // Modified payload should produce different hash
        let req_modified = models::RecoveryBindRequest {
            realm: "azamra".to_string(),
            target_user_id: "usr_123_different".to_string(),
            approval_revision: 1,
            device_id: "dvc_456".to_string(),
            jkt: "jkt_789".to_string(),
            public_jwk,
            binding_operation_id: "op_001".to_string(),
        };

        let hash_mod = compute_recovery_bind_hash(&req_modified);
        assert_ne!(hash1, hash_mod);
    }
}
