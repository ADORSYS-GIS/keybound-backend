CREATE TYPE kyc_document_status AS ENUM (
  'UPLOADED',
  'VERIFIED',
  'REJECTED'
);

CREATE TABLE kyc_document (
  id varchar(40) PRIMARY KEY,
  submission_id varchar(40) NOT NULL REFERENCES kyc_submission(id) ON DELETE CASCADE,
  doc_type varchar(64) NOT NULL,
  s3_bucket varchar(128) NOT NULL,
  s3_key varchar(512) NOT NULL,
  file_name varchar(256) NOT NULL,
  mime_type varchar(128) NOT NULL,
  size_bytes bigint NOT NULL,
  sha256 char(64) NOT NULL,
  status kyc_document_status NOT NULL DEFAULT 'UPLOADED',
  uploaded_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX idx_doc_submission ON kyc_document(submission_id);
CREATE INDEX idx_doc_sha256 ON kyc_document(sha256);
