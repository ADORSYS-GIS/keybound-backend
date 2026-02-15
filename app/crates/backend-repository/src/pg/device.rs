use crate::pg::{PgRepository, PgSqlRepo};
use crate::traits::*;
use backend_model::{db, kc as kc_map};

impl PgRepository {
    pub async fn lookup_device(
        &self,
        req: &kc_map::DeviceLookupRequest,
    ) -> RepoResult<Option<db::DeviceRow>> {
        let row = self
            .lookup_device_db(req.device_id.clone(), req.jkt.clone())
            .await?;
        Ok(row)
    }

    pub async fn list_user_devices(
        &self,
        user_id: &str,
        include_revoked: bool,
    ) -> RepoResult<Vec<db::DeviceRow>> {
        let rows = self
            .list_user_devices_db(user_id.to_owned(), include_revoked)
            .await?;
        Ok(rows)
    }

    pub async fn get_user_device(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> RepoResult<Option<db::DeviceRow>> {
        let row = self
            .get_user_device_db(user_id.to_owned(), device_id.to_owned())
            .await?;
        Ok(row)
    }

    pub async fn update_device_status(
        &self,
        record_id: &str,
        status: &str,
    ) -> RepoResult<db::DeviceRow> {
        let row = self
            .update_device_status_db(record_id.to_owned(), status.to_owned())
            .await?;
        Ok(row)
    }

    pub async fn find_device_binding(
        &self,
        device_id: &str,
        jkt: &str,
    ) -> RepoResult<Option<(String, String)>> {
        let row = self
            .find_device_binding_db(device_id.to_owned(), jkt.to_owned())
            .await?;
        Ok(row)
    }

    pub async fn bind_device(&self, req: &kc_map::EnrollmentBindRequest) -> RepoResult<String> {
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

    pub async fn count_user_devices(&self, user_id: &str) -> RepoResult<i64> {
        let count = self.count_user_devices_db(user_id.to_owned()).await?;
        Ok(count)
    }
}
