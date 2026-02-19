use o2o::o2o;
use std::collections::HashMap;

#[derive(Debug, Clone, o2o)]
#[from_owned(gen_oas_server_staff::models::KycApprovalRequest)]
pub struct KycApprovalRequest {
    pub new_tier: u8,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, o2o)]
#[from_owned(gen_oas_server_staff::models::KycRejectionRequest)]
pub struct KycRejectionRequest {
    pub reason: String,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, o2o)]
#[from_owned(gen_oas_server_staff::models::KycRequestInfoRequest)]
pub struct KycRequestInfoRequest {
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct PresignedPut {
    pub url: String,
    pub method: String,
    pub headers: HashMap<String, String>,
}
