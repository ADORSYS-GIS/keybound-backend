CREATE TABLE kyc_document (
  id varchar(40) PRIMARY KEY,
  submission_id varchar(40) NOT NULL,
  doc_type varchar(64) NOT NULL,
  s3_bucket varchar(128) NOT NULL,
  s3_key varchar(512) NOT NULL,
  file_name varchar(256) NOT NULL,
  mime_type varchar(128) NOT NULL,
  size_bytes bigint NOT NULL,
  sha256 char(64) NOT NULL,
  status varchar(32) NOT NULL DEFAULT 'UPLOADED',
  uploaded_at timestamp with time zone NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT fk_doc_submission FOREIGN KEY (submission_id) REFERENCES kyc_submission(id) ON DELETE CASCADE
);

CREATE INDEX idx_doc_submission ON kyc_document(submission_id);
CREATE INDEX idx_doc_sha256 ON kyc_document(sha256);
