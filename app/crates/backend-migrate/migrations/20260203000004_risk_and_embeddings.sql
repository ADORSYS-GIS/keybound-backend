CREATE EXTENSION IF NOT EXISTS vector;

CREATE TABLE kyc_risk_analysis (
  id varchar(40) PRIMARY KEY,
  submission_id varchar(40) NOT NULL REFERENCES kyc_submission(id) ON DELETE CASCADE,
  model_name varchar(128) NOT NULL,
  model_version varchar(64) NOT NULL,
  risk_score double precision NOT NULL,
  risk_level varchar(16) NOT NULL,
  flags jsonb NOT NULL DEFAULT '[]'::jsonb,
  narrative text,
  features jsonb NOT NULL DEFAULT '{}'::jsonb,
  created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE kyc_embedding (
  submission_id varchar(40) PRIMARY KEY REFERENCES kyc_submission(id) ON DELETE CASCADE,
  text_embedding vector(768),
  document_embedding vector(768),
  device_embedding vector(256),
  created_at timestamptz NOT NULL DEFAULT now()
);
