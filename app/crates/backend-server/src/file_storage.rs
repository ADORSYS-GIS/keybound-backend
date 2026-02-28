use backend_core::Error;
use bytes::Bytes;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct PresignedUpload {
    pub url: String,
    pub headers: HashMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncryptionMode {
    None,
    S3,
    Kms,
}

#[cfg_attr(any(test, feature = "test-utils"), mockall::automock)]
#[backend_core::async_trait]
pub trait FileStorage: Send + Sync {
    async fn head_object(&self, bucket: &str, key: &str) -> Result<(), Error>;

    async fn upload(
        &self,
        bucket: &str,
        key: &str,
        mime_type: &str,
        encryption: EncryptionMode,
        body: Bytes,
    ) -> Result<(), Error>;

    async fn upload_presigned(
        &self,
        bucket: &str,
        key: &str,
        mime_type: &str,
        encryption: EncryptionMode,
        expires_in: Duration,
    ) -> Result<PresignedUpload, Error>;

    async fn download(&self, bucket: &str, key: &str) -> Result<Bytes, Error>;

    async fn download_presigned(
        &self,
        bucket: &str,
        key: &str,
        expires_in: Duration,
        content_disposition: Option<String>,
    ) -> Result<String, Error>;
}

pub struct S3FileStorage {
    client: aws_sdk_s3::Client,
}

impl S3FileStorage {
    pub fn new(client: aws_sdk_s3::Client) -> Self {
        Self { client }
    }
}

#[backend_core::async_trait]
impl FileStorage for S3FileStorage {
    async fn head_object(&self, bucket: &str, key: &str) -> Result<(), Error> {
        self.client
            .head_object()
            .bucket(bucket)
            .key(key)
            .send()
            .await
            .map(|_| ())
            .map_err(|e| Error::s3(e.to_string()))
    }

    async fn upload(
        &self,
        _bucket: &str,
        _key: &str,
        _mime_type: &str,
        _encryption: EncryptionMode,
        _body: Bytes,
    ) -> Result<(), Error> {
        Err(Error::bad_request(
            "STORAGE_UNSUPPORTED",
            "upload is not supported by S3 storage",
        ))
    }

    async fn upload_presigned(
        &self,
        bucket: &str,
        key: &str,
        mime_type: &str,
        encryption: EncryptionMode,
        expires_in: Duration,
    ) -> Result<PresignedUpload, Error> {
        let mut builder = self.client.put_object().bucket(bucket).key(key);
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_owned(), mime_type.to_owned());

        match encryption {
            EncryptionMode::S3 => {
                builder =
                    builder.server_side_encryption(aws_sdk_s3::types::ServerSideEncryption::Aes256);
                headers.insert(
                    "x-amz-server-side-encryption".to_owned(),
                    "AES256".to_owned(),
                );
            }
            EncryptionMode::Kms => {
                builder =
                    builder.server_side_encryption(aws_sdk_s3::types::ServerSideEncryption::AwsKms);
                headers.insert(
                    "x-amz-server-side-encryption".to_owned(),
                    "aws:kms".to_owned(),
                );
            }
            EncryptionMode::None => {}
        }

        let presigned = builder
            .content_type(mime_type)
            .presigned(
                aws_sdk_s3::presigning::PresigningConfig::expires_in(expires_in)
                    .map_err(|e| Error::s3(e.to_string()))?,
            )
            .await
            .map_err(|e| Error::s3(e.to_string()))?;

        Ok(PresignedUpload {
            url: presigned.uri().to_string(),
            headers,
        })
    }

    async fn download(&self, _bucket: &str, _key: &str) -> Result<Bytes, Error> {
        Err(Error::bad_request(
            "STORAGE_UNSUPPORTED",
            "download is not supported by S3 storage",
        ))
    }

    async fn download_presigned(
        &self,
        bucket: &str,
        key: &str,
        expires_in: Duration,
        content_disposition: Option<String>,
    ) -> Result<String, Error> {
        let mut builder = self.client.get_object().bucket(bucket).key(key);
        if let Some(cd) = content_disposition {
            builder = builder.response_content_disposition(cd);
        }

        let presigned_req = builder
            .presigned(
                aws_sdk_s3::presigning::PresigningConfig::expires_in(expires_in)
                    .map_err(|e| Error::s3(e.to_string()))?,
            )
            .await
            .map_err(|e| Error::s3(e.to_string()))?;

        Ok(presigned_req.uri().to_string())
    }
}

pub struct FsFileStorage {
    base_dir: PathBuf,
}

impl FsFileStorage {
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    fn path_for(&self, bucket: &str, key: &str) -> PathBuf {
        // Bucket is mapped to a top-level folder to keep compatibility with the S3 API shape.
        self.base_dir.join(bucket).join(key)
    }

    fn ensure_parent(path: &Path) -> Result<(), Error> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(Error::from)?;
        }
        Ok(())
    }
}

#[backend_core::async_trait]
impl FileStorage for FsFileStorage {
    async fn head_object(&self, bucket: &str, key: &str) -> Result<(), Error> {
        let path = self.path_for(bucket, key);
        if path.exists() {
            Ok(())
        } else {
            Err(Error::not_found("OBJECT_NOT_FOUND", "Object not found"))
        }
    }

    async fn upload(
        &self,
        bucket: &str,
        key: &str,
        _mime_type: &str,
        _encryption: EncryptionMode,
        body: Bytes,
    ) -> Result<(), Error> {
        let path = self.path_for(bucket, key);
        Self::ensure_parent(&path)?;
        std::fs::write(path, body).map_err(Error::from)?;
        Ok(())
    }

    async fn upload_presigned(
        &self,
        _bucket: &str,
        _key: &str,
        _mime_type: &str,
        _encryption: EncryptionMode,
        _expires_in: Duration,
    ) -> Result<PresignedUpload, Error> {
        Err(Error::bad_request(
            "STORAGE_UNSUPPORTED",
            "uploadPresigned is not supported by filesystem storage",
        ))
    }

    async fn download(&self, bucket: &str, key: &str) -> Result<Bytes, Error> {
        let path = self.path_for(bucket, key);
        let data = std::fs::read(path).map_err(Error::from)?;
        Ok(Bytes::from(data))
    }

    async fn download_presigned(
        &self,
        _bucket: &str,
        _key: &str,
        _expires_in: Duration,
        _content_disposition: Option<String>,
    ) -> Result<String, Error> {
        Err(Error::bad_request(
            "STORAGE_UNSUPPORTED",
            "downloadPresigned is not supported by filesystem storage",
        ))
    }
}

