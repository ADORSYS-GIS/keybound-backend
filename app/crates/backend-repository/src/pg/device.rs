use crate::traits::*;
use backend_core::{Error, async_trait};
use backend_model::{db, kc as kc_map};
use diesel::prelude::*;
use diesel::result::{DatabaseErrorKind, Error as DieselError};
use diesel_async::AsyncPgConnection;
use diesel_async::RunQueryDsl;
use diesel_async::pooled_connection::deadpool::Pool;
use tracing::{debug, instrument};

#[derive(Clone)]
pub struct DeviceRepository {
    pub(crate) pool: Pool<AsyncPgConnection>,
}

impl DeviceRepository {
    pub fn new(pool: Pool<AsyncPgConnection>) -> Self {
        Self { pool }
    }

    async fn get_conn(
        &self,
    ) -> RepoResult<diesel_async::pooled_connection::deadpool::Object<AsyncPgConnection>> {
        self.pool
            .get()
            .await
            .map_err(|e| backend_core::Error::DieselPool(e.to_string()))
    }
}

#[async_trait]
impl DeviceRepo for DeviceRepository {
    #[instrument(skip(self))]
    async fn lookup_device(
        &self,
        req: &kc_map::DeviceLookupRequest,
    ) -> RepoResult<Option<db::DeviceRow>> {
        debug!("Looking up device: {:?}", req.device_id);
        use backend_model::schema::device::dsl::*;

        let mut conn = self.get_conn().await?;
        if req.device_id.is_none() && req.jkt.is_none() {
            return Ok(None);
        }

        let mut query = device.into_boxed();

        if let Some(device_id_val) = &req.device_id {
            query = query.filter(device_id.eq(device_id_val));
        }

        if let Some(jkt_val) = &req.jkt {
            query = query.filter(jkt.eq(jkt_val));
        }

        let mut row_opt = query
            .first::<db::DeviceRow>(&mut conn)
            .await
            .optional()
            .map_err(Into::<Error>::into)?;

        if let Some(ref mut row) = row_opt {
            let now = chrono::Utc::now();
            diesel::update(
                device
                    .filter(device_id.eq(&row.device_id))
                    .filter(public_jwk.eq(&row.public_jwk)),
            )
            .set(last_seen_at.eq(now))
            .execute(&mut conn)
            .await
            .map_err(Into::<Error>::into)?;
            row.last_seen_at = Some(now);
        }

        Ok(row_opt)
    }

    async fn list_user_devices(
        &self,
        user_id_val: &str,
        include_revoked: bool,
    ) -> RepoResult<Vec<db::DeviceRow>> {
        use backend_model::schema::device::dsl::*;

        let mut conn = self.get_conn().await?;
        let mut query = device.filter(user_id.eq(user_id_val)).into_boxed();

        if !include_revoked {
            query = query.filter(status.eq("ACTIVE"));
        }

        query
            .load::<db::DeviceRow>(&mut conn)
            .await
            .map_err(Into::<Error>::into)
    }

    async fn get_user_device(
        &self,
        user_id_val: &str,
        device_id_val: &str,
    ) -> RepoResult<Option<db::DeviceRow>> {
        use backend_model::schema::device::dsl::*;

        let mut conn = self.get_conn().await?;

        device
            .filter(user_id.eq(user_id_val))
            .filter(device_id.eq(device_id_val))
            .first::<db::DeviceRow>(&mut conn)
            .await
            .optional()
            .map_err(Into::<Error>::into)
    }

    async fn update_device_status(
        &self,
        record_id_val: &str,
        status_val: &str,
    ) -> RepoResult<db::DeviceRow> {
        use backend_model::schema::device::dsl::*;

        let mut conn = self.get_conn().await?;

        diesel::update(device.filter(device_record_id.eq(record_id_val)))
            .set(status.eq(status_val))
            .get_result::<db::DeviceRow>(&mut conn)
            .await
            .map_err(Into::<Error>::into)
    }

    async fn find_device_binding(
        &self,
        device_id_val: &str,
        jkt_val: &str,
    ) -> RepoResult<Option<(String, String)>> {
        use backend_model::schema::device::dsl::*;

        let mut conn = self.get_conn().await?;

        device
            .filter(device_id.eq(device_id_val))
            .filter(jkt.eq(jkt_val))
            .select((user_id, device_record_id))
            .first::<(String, String)>(&mut conn)
            .await
            .optional()
            .map_err(Into::<Error>::into)
    }

    #[instrument(skip(self))]
    async fn bind_device(&self, req: &kc_map::EnrollmentBindRequest) -> RepoResult<String> {
        debug!("Binding device: {} to user: {}", req.device_id, req.user_id);
        use backend_model::schema::device::dsl::*;

        let mut conn = self.get_conn().await?;

        // Canonicalize by sorting keys and serializing deterministically.
        // This ensures stable device_record_id across platforms and retries.
        let mut sorted_jwk: std::collections::BTreeMap<String, serde_json::Value> =
            std::collections::BTreeMap::new();
        for (k, v) in &req.public_jwk {
            sorted_jwk.insert(k.clone(), v.0.clone());
        }
        let public_jwk_str = serde_json::to_string(&sorted_jwk).map_err(|e| {
            Error::Server(format!(
                "Failed to serialize public_jwk for device bind: {e}"
            ))
        })?;

        let record_id = backend_model::kc::device_record_id(&req.device_id, &public_jwk_str);
        let now = chrono::Utc::now();
        let new_device = db::DeviceRow {
            device_id: req.device_id.clone(),
            user_id: req.user_id.clone(),
            jkt: req.jkt.clone(),
            public_jwk: public_jwk_str.clone(),
            device_record_id: record_id.clone(),
            status: "ACTIVE".to_string(),
            label: None, // Label is not provided in EnrollmentBindRequest
            created_at: now,
            last_seen_at: Some(now),
        };

        // Enforce uniqueness on both device_id and jkt deterministically.
        // Even with API-level prechecks, we can still race at write time.
        let conflicting = device
            .filter(device_id.eq(&req.device_id).or(jkt.eq(&req.jkt)))
            .select(db::DeviceRow::as_select())
            .first::<db::DeviceRow>(&mut conn)
            .await
            .optional()
            .map_err(Into::<Error>::into)?;
        if let Some(existing) = conflicting
            && existing.user_id != req.user_id
        {
            return Err(Error::conflict(
                "DEVICE_ALREADY_BOUND",
                "Device already bound to a different user",
            ));
        }

        let insert_res = diesel::insert_into(device)
            .values(&new_device)
            .execute(&mut conn)
            .await;

        if let Err(DieselError::DatabaseError(DatabaseErrorKind::UniqueViolation, _)) = insert_res {
            // Resolve race: read the winning row and decide deterministically.
            let winner = device
                .filter(device_id.eq(&req.device_id).or(jkt.eq(&req.jkt)))
                .select(db::DeviceRow::as_select())
                .first::<db::DeviceRow>(&mut conn)
                .await
                .optional()
                .map_err(Into::<Error>::into)?;

            if let Some(winner) = winner {
                if winner.user_id != req.user_id {
                    return Err(Error::conflict(
                        "DEVICE_ALREADY_BOUND",
                        "Device already bound to a different user",
                    ));
                }
                return Ok(winner.device_record_id);
            }
        } else {
            insert_res.map_err(Into::<Error>::into)?;
        }

        Ok(record_id)
    }

    async fn count_user_devices(&self, user_id_val: &str) -> RepoResult<i64> {
        use backend_model::schema::device::dsl::*;

        let mut conn = self.get_conn().await?;

        device
            .filter(user_id.eq(user_id_val))
            .count()
            .get_result::<i64>(&mut conn)
            .await
            .map_err(Into::<Error>::into)
    }
}
