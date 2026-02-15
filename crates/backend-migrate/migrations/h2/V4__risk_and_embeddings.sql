CREATE TABLE kyc_risk_analysis (
  id varchar(40) PRIMARY KEY,
  submission_id varchar(40) NOT NULL,
  model_name varchar(128) NOT NULL,
  model_version varchar(64) NOT NULL,
  risk_score double precision NOT NULL,
  risk_level varchar(16) NOT NULL,
  flags clob NOT NULL DEFAULT '[]',
  narrative clob,
  features clob NOT NULL DEFAULT '{}',
  created_at timestamp with time zone NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT fk_risk_submission FOREIGN KEY (submission_id) REFERENCES kyc_submission(id) ON DELETE CASCADE
);

CREATE TABLE kyc_embedding (
  submission_id varchar(40) PRIMARY KEY,
  text_embedding varchar(8192),
  document_embedding varchar(8192),
  device_embedding varchar(4096),
  created_at timestamp with time zone NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT fk_embedding_submission FOREIGN KEY (submission_id) REFERENCES kyc_submission(id) ON DELETE CASCADE
);
