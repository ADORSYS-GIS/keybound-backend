use chrono::{DateTime, Utc};
use diesel::prelude::*;
use serde_json::Value;

#[derive(Debug, Clone, Queryable, Selectable, Insertable)]
#[diesel(table_name = crate::schema::app_user)]
pub struct UserRow {
    pub user_id: String,
    pub realm: String,
    pub username: String,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub email: Option<String>,
    pub email_verified: bool,
    pub phone_number: Option<String>,
    pub fineract_customer_id: Option<String>,
    pub disabled: bool,
    pub attributes: Option<Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Queryable, Selectable, Insertable)]
#[diesel(table_name = crate::schema::device)]
pub struct DeviceRow {
    pub device_id: String,
    pub user_id: String,
    pub jkt: String,
    pub public_jwk: String,
    pub status: String,
    pub label: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_seen_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Queryable, Selectable, Insertable)]
#[diesel(table_name = crate::schema::kyc_session)]
pub struct KycSessionRow {
    pub id: String,
    pub user_id: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Queryable, Selectable, Insertable)]
#[diesel(table_name = crate::schema::kyc_step)]
pub struct KycStepRow {
    pub id: String,
    pub session_id: String,
    pub user_id: String,
    pub step_type: String,
    pub status: String,
    pub data: Value,
    pub policy: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub submitted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Queryable, Selectable, Insertable)]
#[diesel(table_name = crate::schema::kyc_otp_challenge)]
pub struct KycOtpChallengeRow {
    pub otp_ref: String,
    pub step_id: String,
    pub msisdn: String,
    pub channel: String,
    pub otp_hash: String,
    pub expires_at: DateTime<Utc>,
    pub tries_left: i32,
    pub created_at: DateTime<Utc>,
    pub verified_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Queryable, Selectable, Insertable)]
#[diesel(table_name = crate::schema::kyc_magic_email_challenge)]
pub struct KycMagicEmailChallengeRow {
    pub token_ref: String,
    pub step_id: String,
    pub email: String,
    pub token_hash: String,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub verified_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Queryable, Selectable, Insertable)]
#[diesel(table_name = crate::schema::kyc_upload)]
pub struct KycUploadRow {
    pub upload_id: String,
    pub step_id: String,
    pub user_id: String,
    pub purpose: String,
    pub asset_type: String,
    pub mime: String,
    pub size_bytes: i64,
    pub bucket: String,
    pub object_key: String,
    pub method: String,
    pub url: String,
    pub headers: Value,
    pub multipart: Option<Value>,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub etag: Option<String>,
    pub computed_sha256: Option<String>,
}

#[derive(Debug, Clone, Queryable, Selectable, Insertable)]
#[diesel(table_name = crate::schema::kyc_evidence)]
pub struct KycEvidenceRow {
    pub evidence_id: String,
    pub step_id: String,
    pub asset_type: String,
    pub bucket: String,
    pub object_key: String,
    pub sha256: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Queryable, Selectable, Insertable)]
#[diesel(table_name = crate::schema::kyc_review_queue)]
pub struct KycReviewQueueRow {
    pub id: i64,
    pub session_id: String,
    pub step_id: String,
    pub status: String,
    pub assigned_to: Option<String>,
    pub claimed_at: Option<DateTime<Utc>>,
    pub lock_expires_at: Option<DateTime<Utc>>,
    pub priority: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Queryable, Selectable, Insertable)]
#[diesel(table_name = crate::schema::kyc_review_decision)]
pub struct KycReviewDecisionRow {
    pub id: i64,
    pub session_id: String,
    pub step_id: String,
    pub outcome: String,
    pub reason_code: String,
    pub comment: Option<String>,
    pub decided_at: DateTime<Utc>,
    pub reviewer_id: Option<String>,
}

#[derive(Debug, Clone, Queryable, Selectable, Insertable)]
#[diesel(table_name = crate::schema::phone_deposit)]
pub struct PhoneDepositRow {
    pub deposit_id: String,
    pub user_id: String,
    pub amount: f64,
    pub currency: String,
    pub reason: Option<String>,
    pub reference: Option<String>,
    pub status: String,
    pub staff_id: String,
    pub staff_full_name: String,
    pub staff_phone_number: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl diesel::associations::HasTable for UserRow {
    type Table = crate::schema::app_user::table;

    fn table() -> Self::Table {
        crate::schema::app_user::table
    }
}

impl diesel::associations::HasTable for DeviceRow {
    type Table = crate::schema::device::table;

    fn table() -> Self::Table {
        crate::schema::device::table
    }
}

impl diesel::associations::HasTable for KycSessionRow {
    type Table = crate::schema::kyc_session::table;

    fn table() -> Self::Table {
        crate::schema::kyc_session::table
    }
}

impl diesel::associations::HasTable for KycStepRow {
    type Table = crate::schema::kyc_step::table;

    fn table() -> Self::Table {
        crate::schema::kyc_step::table
    }
}

impl diesel::associations::HasTable for KycOtpChallengeRow {
    type Table = crate::schema::kyc_otp_challenge::table;

    fn table() -> Self::Table {
        crate::schema::kyc_otp_challenge::table
    }
}

impl diesel::associations::HasTable for KycMagicEmailChallengeRow {
    type Table = crate::schema::kyc_magic_email_challenge::table;

    fn table() -> Self::Table {
        crate::schema::kyc_magic_email_challenge::table
    }
}

impl diesel::associations::HasTable for KycUploadRow {
    type Table = crate::schema::kyc_upload::table;

    fn table() -> Self::Table {
        crate::schema::kyc_upload::table
    }
}

impl diesel::associations::HasTable for KycEvidenceRow {
    type Table = crate::schema::kyc_evidence::table;

    fn table() -> Self::Table {
        crate::schema::kyc_evidence::table
    }
}

impl diesel::associations::HasTable for KycReviewQueueRow {
    type Table = crate::schema::kyc_review_queue::table;

    fn table() -> Self::Table {
        crate::schema::kyc_review_queue::table
    }
}

impl diesel::associations::HasTable for KycReviewDecisionRow {
    type Table = crate::schema::kyc_review_decision::table;

    fn table() -> Self::Table {
        crate::schema::kyc_review_decision::table
    }
}

impl diesel::associations::HasTable for PhoneDepositRow {
    type Table = crate::schema::phone_deposit::table;

    fn table() -> Self::Table {
        crate::schema::phone_deposit::table
    }
}

impl<'a> diesel::Identifiable for &'a UserRow {
    type Id = &'a str;

    fn id(self) -> Self::Id {
        self.user_id.as_str()
    }
}

impl<'a> diesel::Identifiable for &'a DeviceRow {
    type Id = (&'a str, &'a str);

    fn id(self) -> Self::Id {
        (self.device_id.as_str(), self.public_jwk.as_str())
    }
}

impl<'a> diesel::Identifiable for &'a KycSessionRow {
    type Id = &'a str;

    fn id(self) -> Self::Id {
        self.id.as_str()
    }
}

impl<'a> diesel::Identifiable for &'a KycStepRow {
    type Id = &'a str;

    fn id(self) -> Self::Id {
        self.id.as_str()
    }
}

impl<'a> diesel::Identifiable for &'a KycOtpChallengeRow {
    type Id = &'a str;

    fn id(self) -> Self::Id {
        self.otp_ref.as_str()
    }
}

impl<'a> diesel::Identifiable for &'a KycMagicEmailChallengeRow {
    type Id = &'a str;

    fn id(self) -> Self::Id {
        self.token_ref.as_str()
    }
}

impl<'a> diesel::Identifiable for &'a KycUploadRow {
    type Id = &'a str;

    fn id(self) -> Self::Id {
        self.upload_id.as_str()
    }
}

impl<'a> diesel::Identifiable for &'a KycEvidenceRow {
    type Id = &'a str;

    fn id(self) -> Self::Id {
        self.evidence_id.as_str()
    }
}

impl<'a> diesel::Identifiable for &'a KycReviewQueueRow {
    type Id = i64;

    fn id(self) -> Self::Id {
        self.id
    }
}

impl<'a> diesel::Identifiable for &'a KycReviewDecisionRow {
    type Id = i64;

    fn id(self) -> Self::Id {
        self.id
    }
}

impl<'a> diesel::Identifiable for &'a PhoneDepositRow {
    type Id = &'a str;

    fn id(self) -> Self::Id {
        self.deposit_id.as_str()
    }
}
