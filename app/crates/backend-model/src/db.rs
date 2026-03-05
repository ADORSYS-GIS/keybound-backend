use chrono::{DateTime, Utc};
use diesel::prelude::*;
use serde_json::Value;

#[derive(Debug, Clone, Queryable, Selectable, Insertable)]
#[diesel(table_name = crate::schema::app_user)]
pub struct UserRow {
    pub user_id: String,
    pub realm: String,
    pub username: String,
    pub full_name: Option<String>,
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
    pub device_record_id: String,
    pub status: String,
    pub label: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_seen_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Queryable, Selectable, Insertable)]
#[diesel(table_name = crate::schema::sm_instance)]
pub struct SmInstanceRow {
    pub id: String,
    pub kind: String,
    pub user_id: Option<String>,
    pub idempotency_key: String,
    pub status: String,
    pub context: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Queryable, Selectable, Insertable)]
#[diesel(table_name = crate::schema::sm_event)]
pub struct SmEventRow {
    pub id: String,
    pub instance_id: String,
    pub kind: String,
    pub actor_type: String,
    pub actor_id: Option<String>,
    pub payload: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Queryable, Selectable, Insertable)]
#[diesel(table_name = crate::schema::sm_step_attempt)]
pub struct SmStepAttemptRow {
    pub id: String,
    pub instance_id: String,
    pub step_name: String,
    pub attempt_no: i32,
    pub status: String,
    pub external_ref: Option<String>,
    pub input: Value,
    pub output: Option<Value>,
    pub error: Option<Value>,
    pub queued_at: Option<DateTime<Utc>>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub next_retry_at: Option<DateTime<Utc>>,
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

impl diesel::associations::HasTable for SmInstanceRow {
    type Table = crate::schema::sm_instance::table;

    fn table() -> Self::Table {
        crate::schema::sm_instance::table
    }
}

impl diesel::associations::HasTable for SmEventRow {
    type Table = crate::schema::sm_event::table;

    fn table() -> Self::Table {
        crate::schema::sm_event::table
    }
}

impl diesel::associations::HasTable for SmStepAttemptRow {
    type Table = crate::schema::sm_step_attempt::table;

    fn table() -> Self::Table {
        crate::schema::sm_step_attempt::table
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

impl<'a> diesel::Identifiable for &'a SmInstanceRow {
    type Id = &'a str;

    fn id(self) -> Self::Id {
        self.id.as_str()
    }
}

impl<'a> diesel::Identifiable for &'a SmEventRow {
    type Id = &'a str;

    fn id(self) -> Self::Id {
        self.id.as_str()
    }
}

impl<'a> diesel::Identifiable for &'a SmStepAttemptRow {
    type Id = &'a str;

    fn id(self) -> Self::Id {
        self.id.as_str()
    }
}
