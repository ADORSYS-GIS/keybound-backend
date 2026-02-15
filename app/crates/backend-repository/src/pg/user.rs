use crate::pg::{PgRepository, PgSqlRepo};
use crate::traits::*;
use backend_model::{db, kc as kc_map};
use serde_json::json;

impl PgRepository {
    pub async fn create_user(&self, req: &kc_map::UserUpsert) -> RepoResult<db::UserRow> {
        let user_id = backend_id::user_id()?;
        let attributes_json = req
            .attributes
            .clone()
            .map(|m| serde_json::to_value(m).unwrap_or_default());

        let row = self
            .create_user_db(
                user_id,
                req.realm.clone(),
                req.username.clone(),
                req.first_name.clone(),
                req.last_name.clone(),
                req.email.clone(),
                req.enabled.unwrap_or(true),
                req.email_verified.unwrap_or(false),
                attributes_json,
            )
            .await?;
        Ok(row)
    }

    pub async fn get_user(&self, user_id: &str) -> RepoResult<Option<db::UserRow>> {
        let row = self.get_user_db(user_id.to_owned()).await?;
        Ok(row)
    }

    pub async fn update_user(
        &self,
        user_id: &str,
        req: &kc_map::UserUpsert,
    ) -> RepoResult<Option<db::UserRow>> {
        let attributes_json = req
            .attributes
            .clone()
            .map(|m| serde_json::to_value(m).unwrap_or_default());

        let row = self
            .update_user_db(
                user_id.to_owned(),
                req.realm.clone(),
                req.username.clone(),
                req.first_name.clone(),
                req.last_name.clone(),
                req.email.clone(),
                req.enabled.unwrap_or(true),
                req.email_verified.unwrap_or(false),
                attributes_json,
            )
            .await?;
        Ok(row)
    }

    pub async fn delete_user(&self, user_id: &str) -> RepoResult<u64> {
        let res = self.delete_user_db(user_id.to_owned()).await?;
        Ok(res.rows_affected())
    }

    pub async fn search_users(&self, req: &kc_map::UserSearch) -> RepoResult<Vec<db::UserRow>> {
        let max_results = req.max_results.unwrap_or(50).clamp(1, 200);
        let first_result = req.first_result.unwrap_or(0).max(0);

        let rows = self
            .search_users_db(
                req.realm.clone(),
                req.search.clone(),
                req.username.clone(),
                req.email.clone(),
                req.enabled,
                req.email_verified,
                max_results,
                first_result,
            )
            .await?;
        Ok(rows)
    }

    pub async fn resolve_user_by_phone(
        &self,
        realm: &str,
        phone: &str,
    ) -> RepoResult<Option<db::UserRow>> {
        let cache_key = Self::phone_cache_key(realm, phone);
        {
            let mut cache = self
                .resolve_user_by_phone_cache
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if let Some(cached) = cache.get(&cache_key).cloned() {
                return Ok(cached);
            }
        }

        let user = self
            .resolve_user_by_phone_db(realm.to_owned(), phone.to_owned())
            .await?;

        {
            let mut cache = self
                .resolve_user_by_phone_cache
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            cache.put(cache_key, user.clone());
        }

        Ok(user)
    }

    pub async fn resolve_or_create_user_by_phone(
        &self,
        realm: &str,
        phone: &str,
    ) -> RepoResult<(db::UserRow, bool)> {
        if let Some(user) = self.resolve_user_by_phone(realm, phone).await? {
            return Ok((user, false));
        }

        let user_id = backend_id::user_id()?;
        let attributes_json = json!({ "phone_number": phone });
        let user = self
            .create_user_by_phone_db(user_id, realm.to_owned(), phone.to_owned(), attributes_json)
            .await?;

        {
            let mut cache = self
                .resolve_user_by_phone_cache
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            cache.put(Self::phone_cache_key(realm, phone), Some(user.clone()));
        }

        Ok((user, true))
    }
}
