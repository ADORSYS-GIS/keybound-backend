use chrono::{DateTime, Utc};
use std::collections::HashMap;

use crate::db;
use gen_oas_server_bff::models;

#[derive(Debug, Clone)]
pub struct KycDocumentUploadRequest {
    pub document_type: String,
    pub file_name: String,
    pub mime_type: String,
    pub content_length: i64,
}

impl From<models::KycDocumentUploadRequest> for KycDocumentUploadRequest {
    fn from(req: models::KycDocumentUploadRequest) -> Self {
        Self {
            document_type: req.document_type,
            file_name: req.file_name,
            mime_type: req.mime_type,
            content_length: req.content_length,
        }
    }
}

#[derive(Debug, Clone)]
pub struct KycInformationPatchRequest {
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub email: Option<String>,
    pub phone_number: Option<String>,
    pub date_of_birth: Option<String>,
    pub nationality: Option<String>,
}

impl From<models::KycCasePatchRequest> for KycInformationPatchRequest {
    fn from(req: models::KycCasePatchRequest) -> Self {
        let p = req.user_profile.as_ref();
        Self {
            first_name: p.and_then(|x| x.first_name.clone()),
            last_name: p.and_then(|x| x.last_name.clone()),
            email: p.and_then(|x| x.email.clone()),
            phone_number: p.and_then(|x| x.phone_number.clone()),
            date_of_birth: p.and_then(|x| x.date_of_birth.map(|d| d.to_string())),
            nationality: p.and_then(|x| x.nationality.clone()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct KycStatusDocumentStatusDto {
    pub document_type: Option<String>,
    pub status: Option<String>,
    pub uploaded_at: Option<DateTime<Utc>>,
    pub rejection_reason: Option<String>,
}

impl From<db::KycDocumentRow> for KycStatusDocumentStatusDto {
    fn from(row: db::KycDocumentRow) -> Self {
        Self {
            document_type: Some(row.document_type),
            status: Some(row.status),
            uploaded_at: row.uploaded_at,
            rejection_reason: row.rejection_reason,
        }
    }
}

impl Into<models::KycDocument> for KycStatusDocumentStatusDto {
    fn into(self) -> models::KycDocument {
        models::KycDocument {
            document_type: self.document_type,
            status: self.status,
            uploaded_at: self.uploaded_at,
            document_id: None,
            file_name: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct KycDocumentUploadResponseDto {
    pub document_id: Option<String>,
    pub document_type: Option<String>,
    pub status: Option<String>,
    pub uploaded_at: Option<DateTime<Utc>>,
    pub file_name: Option<String>,
    pub mime_type: Option<String>,
    pub upload_url: Option<String>,
    pub upload_method: Option<String>,
    pub upload_headers: Option<HashMap<String, String>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub s3_bucket: Option<String>,
    pub s3_key: Option<String>,
}

impl Into<models::KycDocumentUploadResponse> for KycDocumentUploadResponseDto {
    fn into(self) -> models::KycDocumentUploadResponse {
        models::KycDocumentUploadResponse {
            document_id: self.document_id,
            document_type: self.document_type,
            status: self.status,
            uploaded_at: self.uploaded_at,
            file_name: self.file_name,
            mime_type: self.mime_type,
            upload_url: self.upload_url,
            upload_method: self.upload_method,
            upload_headers: self.upload_headers,
            expires_at: self.expires_at,
            s3_bucket: self.s3_bucket,
            s3_key: self.s3_key,
        }
    }
}

#[derive(Debug, Clone)]
pub struct KycStatusResponseDto {
    pub kyc_tier: Option<i32>,
    pub kyc_status: Option<String>,
    pub documents: Option<Vec<models::KycDocument>>,
    pub required_documents: Option<Vec<String>>,
    pub missing_documents: Option<Vec<String>>,
    pub page: Option<i32>,
    pub page_size: Option<i32>,
    pub total_documents: Option<i32>,
}

impl Into<models::KycCaseResponse> for KycStatusResponseDto {
    fn into(self) -> models::KycCaseResponse {
        let status = self.kyc_status.and_then(|s| s.parse().ok());
        models::KycCaseResponse {
            status,
            documents: self.documents,
            case_id: None,
            user_profile: None,
            comments: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct KycInformationResponseDto {
    pub external_id: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub email: Option<String>,
    pub phone_number: Option<String>,
    pub date_of_birth: Option<String>,
    pub nationality: Option<String>,
    pub updated_at: Option<DateTime<Utc>>,
}

impl Into<models::KycCaseResponse> for KycInformationResponseDto {
    fn into(self) -> models::KycCaseResponse {
        let user_profile = models::UserProfile {
            first_name: self.first_name,
            last_name: self.last_name,
            email: self.email,
            phone_number: self.phone_number,
            date_of_birth: self.date_of_birth.and_then(|d| d.parse().ok()),
            nationality: self.nationality,
            address: None,
        };

        models::KycCaseResponse {
            case_id: self.external_id,
            user_profile: Some(user_profile),
            status: None,
            documents: None,
            comments: None,
        }
    }
}
