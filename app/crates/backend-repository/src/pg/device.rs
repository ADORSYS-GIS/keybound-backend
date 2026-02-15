use crate::traits::*;
use backend_model::{db, kc as kc_map};
use serde_json::Value;
use sqlx_data::{dml, repo};
use sqlx::PgPool;

#[repo]
pub trait PgDeviceRepo {
    #[dml(file = "queries/device/lookup.sql", unchecked)]
    async fn lookup_device_db(
        &self,
        device_id: Option<String>,
        jkt: Option<String>,
    ) -> sqlx_data::Result<Option<db::DeviceRow>>;

    #[dml(file = "queries/device/list_user_devices.sql", unchecked)]
    async fn list_user_devices_db(
        &self,
        user_id: String,
        include_revoked: bool,
    ) -> sqlx_data::Result<Vec<db::DeviceRow>>;

    #[dml(file = "queries/device/get_user_device.sql", unchecked)]
    async fn get_user_device_db(
        &self,
        user_id: String,
        device_id: String,
    ) -> sqlx_data::Result<Option<db::DeviceRow>>;

    #[dml(file = "queries/device/update_status.sql", unchecked)]
    async fn update_device_status_db(
        &self,
        record_id: String,
        status: String,
    ) -> sqlx_data::Result<db::DeviceRow>;

    #[dml(file = "queries/device/find_binding.sql", unchecked)]
    async fn find_device_binding_db(
        &self,
        device_id: String,
        jkt: String,
    ) -> sqlx_data::Result<Option<(String, String)>>;

    #[dml(file = "queries/device/bind.sql", unchecked)]
    async fn bind_device_db(
        &self,
        id: String,
        realm: String,
        client_id: String,
        user_id: String,
        user_hint: Option<String>,
        device_id: String,
        jkt: String,
        public_jwk: Value,
        attributes: Option<Value>,
        proof: Option<Value>,
    ) -> sqlx_data::Result<String>;

    #[dml(file = "queries/device/count_user_devices.sql", unchecked)]
    async fn count_user_devices_db(&self, user_id: String) -> sqlx_data::Result<i64>;
}

#[derive(Clone)]
pub struct DeviceRepository {
    pub(crate) pool: PgPool,
}

impl PgDeviceRepo for DeviceRepository {
    fn get_pool(&self) -> &sqlx_data::Pool {
        &self.pool
    }
}

impl DeviceRepo for DeviceRepository {
    async fn lookup_device(
        &self,
        req: &kc_map::DeviceLookupRequest,
    ) -> RepoResult<Option<db::DeviceRow>> {
        let row = self
            .lookup_device_db(req.device_id.clone(), req.jkt.clone())
            .await?;
        Ok(row)
    }

    async fn list_user_devices(
        &self,
        user_id: &str,
        include_revoked: bool,
    ) -> RepoResult<Vec<db::DeviceRow>> {
        let rows = self
            .list_user_devices_db(user_id.to_owned(), include_revoked)
            .await?;
        Ok(rows)
    }

    async fn get_user_device(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> RepoResult<Option<db::DeviceRow>> {
        let row = self
            .get_user_device_db(user_id.to_owned(), device_id.to_owned())
            .await?;
        Ok(row)
    }

    async fn update_device_status(
        &self,
        record_id: &str,
        status: &str,
    ) -> RepoResult<db::DeviceRow> {
        let row = self
            .update_device_status_db(record_id.to_owned(), status.to_owned())
            .await?;
        Ok(row)
    }

    async fn find_device_binding(
        &self,
        device_id: &str,
        jkt: &str,
    ) -> RepoResult<Option<(String, String)>> {
        let row = self
            .find_device_binding_db(device_id.to_owned(), jkt.to_owned())
            .await?;
        Ok(row)
    }

    async fn bind_device(&self, req: &kc_map::EnrollmentBindRequest) -> RepoResult<String> {
        let record_id = backend_id::device_id()?;
        let attributes_json = req
            .attributes
            .clone()
            .map(|m| serde_json::to_value(m).unwrap_or_default());
        let public_jwk = serde_json::to_value(req.public_jwk.clone()).unwrap_or_default();
        let proof = req
            .proof
            .clone()
            .map(|m| serde_json::to_value(m).unwrap_or_default());

        let id = self
            .bind_device_db(
                record_id,
                req.realm.clone(),
                req.client_id.clone(),
                req.user_id.clone(),
                req.user_hint.clone(),
                req.device_id.clone(),
                req.jkt.clone(),
                public_jwk,
                attributes_json,
                proof,
            )
            .await?;
        Ok(id)
    }

    async fn count_user_devices(&self, user_id: &str) -> RepoResult<i64> {
        let count = self.count_user_devices_db(user_id.to_owned()).await?;
        Ok(count)
    }
}

impl DeviceRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}
