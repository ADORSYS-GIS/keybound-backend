use backend_core::{Error, Result};

pub fn prefixed(prefix: &str) -> Result<String> {
    let id = cuid::cuid1().map_err(|e| Error::Server(e.to_string()))?;
    Ok(format!("{prefix}_{id}"))
}

pub fn user_id() -> Result<String> {
    prefixed("usr")
}

pub fn device_id() -> Result<String> {
    prefixed("dvc")
}

pub fn approval_id() -> Result<String> {
    prefixed("apr")
}

pub fn sms_hash() -> Result<String> {
    prefixed("sms")
}

pub fn sms_id() -> Result<String> {
    prefixed("sms")
}

pub fn kyc_document_id() -> Result<String> {
    prefixed("kyd")
}

pub fn kyc_case_id() -> Result<String> {
    prefixed("kyc")
}

pub fn kyc_submission_id() -> Result<String> {
    prefixed("sub")
}

pub fn kyc_session_id() -> Result<String> {
    prefixed("kys")
}

pub fn kyc_step_id() -> Result<String> {
    prefixed("kysp")
}

pub fn kyc_otp_ref() -> Result<String> {
    prefixed("otp")
}

pub fn kyc_magic_ref() -> Result<String> {
    prefixed("mgc")
}

pub fn kyc_upload_id() -> Result<String> {
    prefixed("upl")
}

pub fn kyc_evidence_id() -> Result<String> {
    prefixed("evi")
}

pub fn phone_deposit_id() -> Result<String> {
    prefixed("dep")
}

pub fn sm_instance_id() -> Result<String> {
    prefixed("smi")
}

pub fn sm_event_id() -> Result<String> {
    prefixed("sme")
}

pub fn sm_attempt_id() -> Result<String> {
    prefixed("sma")
}
