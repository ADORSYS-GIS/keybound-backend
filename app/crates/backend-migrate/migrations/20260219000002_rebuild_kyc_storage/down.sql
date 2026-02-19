-- Roll back to the previous simplified KYC schema shape.

DROP TABLE IF EXISTS fineract_provisioning CASCADE;
DROP TABLE IF EXISTS kyc_review_decision CASCADE;
DROP TABLE IF EXISTS kyc_review_queue CASCADE;
DROP TABLE IF EXISTS kyc_document CASCADE;
DROP TABLE IF EXISTS kyc_submission_profile_history CASCADE;
DROP TABLE IF EXISTS kyc_submission CASCADE;
DROP TABLE IF EXISTS kyc_case CASCADE;

DROP TYPE IF EXISTS provisioning_status CASCADE;
DROP TYPE IF EXISTS kyc_review_decision_outcome CASCADE;
DROP TYPE IF EXISTS kyc_review_queue_status CASCADE;
DROP TYPE IF EXISTS kyc_document_status CASCADE;
DROP TYPE IF EXISTS kyc_provisioning_status CASCADE;
DROP TYPE IF EXISTS kyc_submission_status CASCADE;
DROP TYPE IF EXISTS kyc_case_status CASCADE;

CREATE TYPE kyc_case_status AS ENUM ('OPEN','CLOSED');

CREATE TYPE kyc_submission_status AS ENUM (
  'DRAFT',
  'SUBMITTED',
  'IN_VERIFICATION',
  'RISK_ASSESSMENT',
  'PENDING_MANUAL_REVIEW',
  'PENDING_USER_RESPONSE',
  'APPROVED',
  'REJECTED'
);

CREATE TYPE kyc_provisioning_status AS ENUM (
  'NONE',
  'STARTED',
  'SUCCEEDED',
  'FAILED'
);

CREATE TYPE kyc_document_status AS ENUM (
  'UPLOADED',
  'VERIFIED',
  'REJECTED'
);

CREATE TYPE provisioning_status AS ENUM (
  'STARTED',
  'SUCCEEDED',
  'FAILED'
);

CREATE TABLE kyc_case (
  id text PRIMARY KEY,
  user_id text NOT NULL REFERENCES app_user(user_id),
  case_status kyc_case_status NOT NULL DEFAULT 'OPEN',
  active_submission_id text,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now(),
  UNIQUE(user_id)
);

CREATE TABLE kyc_submission (
  id text PRIMARY KEY,
  kyc_case_id text NOT NULL REFERENCES kyc_case(id) ON DELETE CASCADE,
  status kyc_submission_status NOT NULL DEFAULT 'DRAFT',
  submitted_at timestamptz,
  decided_at timestamptz,
  decided_by text,
  provisioning_status kyc_provisioning_status NOT NULL DEFAULT 'NONE',
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now(),
  first_name text,
  last_name text,
  email text,
  phone_number text,
  date_of_birth text,
  nationality text,
  rejection_reason text,
  review_notes text
);

ALTER TABLE kyc_case
  ADD CONSTRAINT fk_active_submission
  FOREIGN KEY (active_submission_id)
  REFERENCES kyc_submission(id);

CREATE TABLE kyc_document (
  id text PRIMARY KEY,
  submission_id text NOT NULL REFERENCES kyc_submission(id) ON DELETE CASCADE,
  doc_type text NOT NULL,
  s3_bucket text NOT NULL,
  s3_key text NOT NULL,
  file_name text NOT NULL,
  mime_type text NOT NULL,
  size_bytes bigint NOT NULL,
  sha256 char(64) NOT NULL,
  status kyc_document_status NOT NULL DEFAULT 'UPLOADED',
  uploaded_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE fineract_provisioning (
  id text PRIMARY KEY,
  kyc_case_id text NOT NULL REFERENCES kyc_case(id),
  submission_id text NOT NULL REFERENCES kyc_submission(id),
  status provisioning_status NOT NULL,
  fineract_customer_id text,
  error_code text,
  error_message text,
  attempt_no int NOT NULL DEFAULT 1,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX idx_kyc_case_active_submission ON kyc_case(active_submission_id);
CREATE INDEX idx_kyc_submission_status ON kyc_submission(status);
CREATE INDEX idx_doc_submission ON kyc_document(submission_id);
CREATE INDEX idx_doc_sha256 ON kyc_document(sha256);
