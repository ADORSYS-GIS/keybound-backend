use crate::traits::*;
use backend_core::{Error, async_trait};
use backend_model::{db, schema};
use chrono::{DateTime, Utc};
use diesel::dsl::count_star;
use diesel::prelude::*;
use diesel_async::pooled_connection::deadpool::Pool;
use diesel_async::{AsyncConnection, AsyncPgConnection, RunQueryDsl};
use serde_json::Value;
use tracing::{debug, instrument};

#[derive(Clone)]
pub struct RecoveryCaseRepository {
    pub(crate) pool: Pool<AsyncPgConnection>,
}

impl RecoveryCaseRepository {
    pub fn new(pool: Pool<AsyncPgConnection>) -> Self {
        Self { pool }
    }

    async fn get_conn(
        &self,
    ) -> RepoResult<diesel_async::pooled_connection::deadpool::Object<AsyncPgConnection>> {
        self.pool
            .get()
            .await
            .map_err(|e| Error::DieselPool(e.to_string()))
    }
}

/// Partial, versioned changeset for the recovery_case table. `Option<Option<T>>`
/// semantics: outer `None` skips the column, `Some(None)` writes NULL,
/// `Some(Some(v))` writes the value.
#[derive(Debug, Clone, Default, AsChangeset)]
#[diesel(table_name = schema::recovery_case)]
#[diesel(treat_none_as_null = false)]
struct CaseChangeset {
    session_id: Option<Option<String>>,
    requested_phone_hash: Option<String>,
    requested_phone_masked: Option<String>,
    device_id: Option<Option<String>>,
    jkt: Option<Option<String>>,
    device_public_jwk: Option<Option<Value>>,
    reason: Option<Option<String>>,
    status: Option<String>,
    phone_relation: Option<Option<String>>,
    matched_user_id: Option<Option<String>>,
    otp_hash: Option<Option<String>>,
    otp_expires_at: Option<Option<DateTime<Utc>>>,
    otp_attempts: Option<i32>,
    otp_resend_at: Option<Option<DateTime<Utc>>>,
    review_decision: Option<Option<String>>,
    review_reason: Option<Option<String>>,
    review_checklist: Option<Option<Value>>,
    review_expected_version: Option<Option<i64>>,
    approval_revision: Option<Option<i64>>,
    evidence: Option<Value>,
    old_devices: Option<Value>,
    risk_flags: Option<Value>,
    expires_at: Option<Option<DateTime<Utc>>>,
    review_expires_at: Option<Option<DateTime<Utc>>>,
    approved_expires_at: Option<Option<DateTime<Utc>>>,
}

fn changeset(patch: RecoveryCaseUpdate) -> CaseChangeset {
    CaseChangeset {
        session_id: patch.session_id,
        requested_phone_hash: patch.requested_phone_hash,
        requested_phone_masked: patch.requested_phone_masked,
        device_id: patch.device_id,
        jkt: patch.jkt,
        device_public_jwk: patch.device_public_jwk,
        reason: patch.reason,
        status: patch.status,
        phone_relation: patch.phone_relation,
        matched_user_id: patch.matched_user_id,
        otp_hash: patch.otp_hash,
        otp_expires_at: patch.otp_expires_at,
        otp_attempts: patch.otp_attempts,
        otp_resend_at: patch.otp_resend_at,
        review_decision: patch.review_decision,
        review_reason: patch.review_reason,
        review_checklist: patch.review_checklist,
        review_expected_version: patch.review_expected_version,
        approval_revision: patch.approval_revision,
        evidence: patch.evidence,
        old_devices: patch.old_devices,
        risk_flags: patch.risk_flags,
        expires_at: patch.expires_at,
        review_expires_at: patch.review_expires_at,
        approved_expires_at: patch.approved_expires_at,
    }
}

fn row(input: RecoveryCaseCreateInput) -> db::RecoveryCaseRow {
    db::RecoveryCaseRow {
        id: input.id,
        human_id: input.human_id,
        session_id: input.session_id,
        device_id: input.device_id,
        jkt: input.jkt,
        device_public_jwk: input.device_public_jwk,
        requested_phone_hash: input.requested_phone_hash,
        requested_phone_masked: input.requested_phone_masked,
        reason: input.reason,
        status: input.status,
        phone_relation: input.phone_relation,
        matched_user_id: input.matched_user_id,
        otp_hash: None,
        otp_expires_at: None,
        otp_attempts: 0,
        otp_resend_at: None,
        review_decision: None,
        review_reason: None,
        review_checklist: None,
        review_expected_version: None,
        approval_revision: None,
        evidence: serde_json::json!({}),
        old_devices: serde_json::json!([]),
        risk_flags: serde_json::json!({}),
        expires_at: input.expires_at,
        review_expires_at: None,
        approved_expires_at: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        version: 1,
    }
}

#[async_trait]
impl RecoveryCaseRepo for RecoveryCaseRepository {
    #[instrument(skip(self, input))]
    async fn create_case(&self, input: RecoveryCaseCreateInput) -> RepoResult<db::RecoveryCaseRow> {
        debug!("Creating recovery case: {}", input.id);
        use schema::recovery_case::dsl::*;

        let mut conn = self.get_conn().await?;
        let row = row(input);

        diesel::insert_into(recovery_case)
            .values(&row)
            .returning(db::RecoveryCaseRow::as_returning())
            .get_result::<db::RecoveryCaseRow>(&mut conn)
            .await
            .map_err(Error::from)
    }

    async fn get_case_by_id(&self, case_id_val: &str) -> RepoResult<Option<db::RecoveryCaseRow>> {
        use schema::recovery_case::dsl::*;
        let mut conn = self.get_conn().await?;

        recovery_case
            .filter(id.eq(case_id_val))
            .first::<db::RecoveryCaseRow>(&mut conn)
            .await
            .optional()
            .map_err(Error::from)
    }

    async fn get_case_by_human_id(
        &self,
        human_id_val: &str,
    ) -> RepoResult<Option<db::RecoveryCaseRow>> {
        use schema::recovery_case::dsl::*;
        let mut conn = self.get_conn().await?;

        recovery_case
            .filter(human_id.eq(human_id_val))
            .first::<db::RecoveryCaseRow>(&mut conn)
            .await
            .optional()
            .map_err(Error::from)
    }

    async fn get_case_by_session_id(
        &self,
        session_id_val: &str,
    ) -> RepoResult<Option<db::RecoveryCaseRow>> {
        use schema::recovery_case::dsl::*;
        let mut conn = self.get_conn().await?;

        recovery_case
            .filter(session_id.eq(session_id_val))
            .first::<db::RecoveryCaseRow>(&mut conn)
            .await
            .optional()
            .map_err(Error::from)
    }

    async fn get_case_by_phone_hash(
        &self,
        phone_hash: &str,
    ) -> RepoResult<Option<db::RecoveryCaseRow>> {
        use schema::recovery_case::dsl::*;
        let mut conn = self.get_conn().await?;

        recovery_case
            .filter(requested_phone_hash.eq(phone_hash))
            .order_by(created_at.desc())
            .first::<db::RecoveryCaseRow>(&mut conn)
            .await
            .optional()
            .map_err(Error::from)
    }

    #[instrument(skip(self, patch))]
    async fn update_case(
        &self,
        case_id_val: &str,
        expected_version: i64,
        patch: &RecoveryCaseUpdate,
    ) -> RepoResult<db::RecoveryCaseRow> {
        use schema::recovery_case::dsl::*;

        let mut conn = self.get_conn().await?;

        conn.transaction::<_, Error, _>(|conn| {
            Box::pin(async move {
                let target = recovery_case.filter(id.eq(case_id_val));

                let updated = diesel::update(target)
                    .filter(version.eq(expected_version))
                    .set((
                        &changeset(patch.clone()),
                        updated_at.eq(Utc::now()),
                        version.eq(version + 1),
                    ))
                    .returning(db::RecoveryCaseRow::as_returning())
                    .get_result::<db::RecoveryCaseRow>(conn)
                    .await
                    .optional()
                    .map_err(Error::from)?;

                let Some(updated) = updated else {
                    return Err(Error::conflict(
                        "RECOVERY_CASE_VERSION_CONFLICT",
                        format!(
                            "Recovery case {} modified concurrently (expected version {expected_version})",
                            case_id_val
                        ),
                    ));
                };

                Ok(updated)
            })
        })
        .await
    }

    async fn list_cases(
        &self,
        filter: RecoveryCaseFilter,
    ) -> RepoResult<(Vec<db::RecoveryCaseRow>, i64)> {
        use schema::recovery_case::dsl::*;

        let filter = filter.normalized();
        let mut conn = self.get_conn().await?;

        let mut count_query = recovery_case.into_boxed();
        let mut rows_query = recovery_case.into_boxed();

        if let Some(status_val) = filter.status.as_ref() {
            count_query = count_query.filter(status.eq(status_val));
            rows_query = rows_query.filter(status.eq(status_val));
        }
        if let Some(user_id) = filter.matched_user_id.as_ref() {
            count_query = count_query.filter(matched_user_id.eq(user_id));
            rows_query = rows_query.filter(matched_user_id.eq(user_id));
        }

        let total = count_query
            .select(count_star())
            .get_result::<i64>(&mut conn)
            .await
            .map_err(Error::from)?;

        let rows = rows_query
            .order_by(created_at.desc())
            .limit(i64::from(filter.limit))
            .offset(filter.offset())
            .select(db::RecoveryCaseRow::as_select())
            .load::<db::RecoveryCaseRow>(&mut conn)
            .await
            .map_err(Error::from)?;

        Ok((rows, total))
    }
}
