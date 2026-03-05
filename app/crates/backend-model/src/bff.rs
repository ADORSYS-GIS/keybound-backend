#[derive(Debug, Clone)]
pub struct KycInformationPatchRequest {
    pub full_name: Option<String>,
    pub email: Option<String>,
    pub phone_number: Option<String>,
    pub date_of_birth: Option<String>,
    pub nationality: Option<String>,
}
