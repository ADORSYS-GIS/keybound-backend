use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Display, EnumString, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum SmKind {
    KycPhoneOtp,
    KycEmailMagic,
    KycFirstDeposit,
}

impl SmKind {
    pub const PHONE_OTP: Self = Self::KycPhoneOtp;
    pub const EMAIL_MAGIC: Self = Self::KycEmailMagic;
    pub const FIRST_DEPOSIT: Self = Self::KycFirstDeposit;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Display, EnumString, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum InstanceStatus {
    Active,
    WaitingInput,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl InstanceStatus {
    pub const ACTIVE: Self = Self::Active;
    pub const WAITING_INPUT: Self = Self::WaitingInput;
    pub const RUNNING: Self = Self::Running;
    pub const COMPLETED: Self = Self::Completed;
    pub const FAILED: Self = Self::Failed;
    pub const CANCELLED: Self = Self::Cancelled;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Display, EnumString, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum AttemptStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
}

impl AttemptStatus {
    pub const QUEUED: Self = Self::Queued;
    pub const RUNNING: Self = Self::Running;
    pub const SUCCEEDED: Self = Self::Succeeded;
    pub const FAILED: Self = Self::Failed;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Display, EnumString, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum StepName {
    IssueOtp,
    VerifyOtp,
    MarkComplete,
    AwaitPaymentConfirmation,
    AwaitApproval,
    RegisterCustomer,
    ApproveAndDeposit,
}

impl StepName {
    pub const PHONE_ISSUE_OTP: Self = Self::IssueOtp;
    pub const PHONE_VERIFY_OTP: Self = Self::VerifyOtp;
    pub const MARK_COMPLETE: Self = Self::MarkComplete;
    pub const DEPOSIT_AWAIT_PAYMENT: Self = Self::AwaitPaymentConfirmation;
    pub const DEPOSIT_AWAIT_APPROVAL: Self = Self::AwaitApproval;
    pub const DEPOSIT_REGISTER_CUSTOMER: Self = Self::RegisterCustomer;
    pub const DEPOSIT_APPROVE_AND_DEPOSIT: Self = Self::ApproveAndDeposit;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ActorType {
    User,
    Staff,
    System,
}

impl ActorType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ActorType::User => "USER",
            ActorType::Staff => "STAFF",
            ActorType::System => "SYSTEM",
        }
    }
}

pub const KIND_KYC_PHONE_OTP: &str = "KYC_PHONE_OTP";
pub const KIND_KYC_EMAIL_MAGIC: &str = "KYC_EMAIL_MAGIC";
pub const KIND_KYC_FIRST_DEPOSIT: &str = "KYC_FIRST_DEPOSIT";

pub const INSTANCE_STATUS_ACTIVE: &str = "ACTIVE";
pub const INSTANCE_STATUS_WAITING_INPUT: &str = "WAITING_INPUT";
pub const INSTANCE_STATUS_RUNNING: &str = "RUNNING";
pub const INSTANCE_STATUS_COMPLETED: &str = "COMPLETED";
pub const INSTANCE_STATUS_FAILED: &str = "FAILED";
pub const INSTANCE_STATUS_CANCELLED: &str = "CANCELLED";

pub const ATTEMPT_STATUS_QUEUED: &str = "QUEUED";
pub const ATTEMPT_STATUS_RUNNING: &str = "RUNNING";
pub const ATTEMPT_STATUS_SUCCEEDED: &str = "SUCCEEDED";
pub const ATTEMPT_STATUS_FAILED: &str = "FAILED";

pub const STEP_PHONE_ISSUE_OTP: &str = "ISSUE_OTP";
pub const STEP_PHONE_VERIFY_OTP: &str = "VERIFY_OTP";
pub const STEP_MARK_COMPLETE: &str = "MARK_COMPLETE";

pub const STEP_DEPOSIT_AWAIT_PAYMENT: &str = "AWAIT_PAYMENT_CONFIRMATION";
pub const STEP_DEPOSIT_AWAIT_APPROVAL: &str = "AWAIT_APPROVAL";
pub const STEP_DEPOSIT_REGISTER_CUSTOMER: &str = "REGISTER_CUSTOMER";
pub const STEP_DEPOSIT_APPROVE_AND_DEPOSIT: &str = "APPROVE_AND_DEPOSIT";
