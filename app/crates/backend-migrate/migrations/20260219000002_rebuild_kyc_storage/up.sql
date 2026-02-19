-- Rebuild KYC storage from scratch for BFF and staff flows.
-- Data is intentionally dropped as part of this migration.

DROP TABLE IF EXISTS fineract_provisioning CASCADE;
DROP TABLE IF EXISTS kyc_document CASCADE;
DROP TABLE IF EXISTS kyc_submission_profile_history CASCADE;
DROP TABLE IF EXISTS kyc_review_decision CASCADE;
DROP TABLE IF EXISTS kyc_review_queue CASCADE;
DROP TABLE IF EXISTS kyc_submission CASCADE;
DROP TABLE IF EXISTS kyc_case CASCADE;

DROP TYPE IF EXISTS provisioning_status CASCADE;
DROP TYPE IF EXISTS kyc_document_status CASCADE;
DROP TYPE IF EXISTS kyc_provisioning_status CASCADE;
DROP TYPE IF EXISTS kyc_submission_status CASCADE;
DROP TYPE IF EXISTS kyc_case_status CASCADE;
DROP TYPE IF EXISTS kyc_review_queue_status CASCADE;
DROP TYPE IF EXISTS kyc_review_decision_outcome CASCADE;

CREATE TYPE kyc_case_status AS ENUM ('OPEN', 'CLOSED');

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
  'PENDING',
  'STARTED',
  'SUCCEEDED',
  'FAILED'
);

CREATE TYPE kyc_document_status AS ENUM (
  'INITIATED',
  'UPLOADED',
  'VERIFIED',
  'REJECTED'
);

CREATE TYPE provisioning_status AS ENUM (
  'STARTED',
  'SUCCEEDED',
  'FAILED'
);

CREATE TYPE kyc_review_queue_status AS ENUM (
  'PENDING',
  'CLAIMED',
  'DONE'
);

CREATE TYPE kyc_review_decision_outcome AS ENUM (
  'APPROVE',
  'REJECT'
);

CREATE TABLE kyc_case (
  id text PRIMARY KEY,
  user_id text NOT NULL REFERENCES app_user(user_id),
  case_status kyc_case_status NOT NULL DEFAULT 'OPEN',
  active_submission_id text,
  queue_rank bigint,
  last_activity_at timestamptz,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now(),
  UNIQUE (user_id)
);

CREATE TABLE kyc_submission (
  id text PRIMARY KEY,
  kyc_case_id text NOT NULL REFERENCES kyc_case(id) ON DELETE CASCADE,
  version int NOT NULL DEFAULT 1,
  status kyc_submission_status NOT NULL DEFAULT 'DRAFT',
  provisioning_status kyc_provisioning_status NOT NULL DEFAULT 'NONE',
  requested_tier int,
  decided_tier int,
  approved_tier int,
  submitted_at timestamptz,
  decided_at timestamptz,
  decided_by text,
  reviewer_id text,
  review_notes text,
  rejection_reason text,
  next_action text,
  profile_json jsonb NOT NULL DEFAULT '{}'::jsonb,
  profile_etag text NOT NULL DEFAULT md5(random()::text || clock_timestamp()::text),
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now(),
  first_name text,
  last_name text,
  email text,
  phone_number text,
  date_of_birth text,
  nationality text
);

ALTER TABLE kyc_case
  ADD CONSTRAINT fk_active_submission
  FOREIGN KEY (active_submission_id)
  REFERENCES kyc_submission(id);

CREATE TABLE kyc_submission_profile_history (
  id bigserial PRIMARY KEY,
  submission_id text NOT NULL REFERENCES kyc_submission(id) ON DELETE CASCADE,
  version int NOT NULL,
  profile_json jsonb NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  UNIQUE (submission_id, version)
);

CREATE TABLE kyc_document (
  id text PRIMARY KEY,
  submission_id text NOT NULL REFERENCES kyc_submission(id) ON DELETE CASCADE,
  doc_type text NOT NULL,
  s3_bucket text NOT NULL,
  s3_key text NOT NULL,
  file_name text NOT NULL,
  mime_type text NOT NULL,
  size_bytes bigint NOT NULL,
  sha256 text NOT NULL,
  status kyc_document_status NOT NULL DEFAULT 'INITIATED',
  uploaded_at timestamptz NOT NULL DEFAULT now(),
  expires_at timestamptz,
  object_url text,
  is_verified bool NOT NULL DEFAULT false
);

CREATE TABLE kyc_review_queue (
  id bigserial PRIMARY KEY,
  case_id text NOT NULL REFERENCES kyc_case(id) ON DELETE CASCADE,
  status kyc_review_queue_status NOT NULL DEFAULT 'PENDING',
  assigned_to text,
  claimed_at timestamptz,
  lock_expires_at timestamptz,
  priority int NOT NULL DEFAULT 100,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now(),
  UNIQUE (case_id)
);

CREATE TABLE kyc_review_decision (
  id bigserial PRIMARY KEY,
  submission_id text NOT NULL REFERENCES kyc_submission(id) ON DELETE CASCADE,
  decision kyc_review_decision_outcome NOT NULL,
  reason_code text NOT NULL,
  comment text,
  decided_at timestamptz NOT NULL DEFAULT now(),
  reviewer_id text
);

CREATE TABLE fineract_provisioning (
  id text PRIMARY KEY,
  kyc_case_id text NOT NULL REFERENCES kyc_case(id) ON DELETE CASCADE,
  submission_id text NOT NULL REFERENCES kyc_submission(id) ON DELETE CASCADE,
  status provisioning_status NOT NULL,
  fineract_customer_id text,
  error_code text,
  error_message text,
  attempt_no int NOT NULL DEFAULT 1,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX idx_kyc_case_active_submission ON kyc_case(active_submission_id);
CREATE INDEX idx_kyc_case_status_activity ON kyc_case(case_status, last_activity_at DESC);
CREATE INDEX idx_kyc_submission_case_status ON kyc_submission(kyc_case_id, status);
CREATE INDEX idx_kyc_submission_status_updated ON kyc_submission(status, updated_at DESC);
CREATE INDEX idx_kyc_submission_profile_etag ON kyc_submission(profile_etag);
CREATE INDEX idx_kyc_submission_history_submission ON kyc_submission_profile_history(submission_id, created_at DESC);
CREATE INDEX idx_doc_submission ON kyc_document(submission_id);
CREATE INDEX idx_doc_status_uploaded ON kyc_document(status, uploaded_at DESC);
CREATE INDEX idx_review_queue_status_priority ON kyc_review_queue(status, priority DESC, created_at ASC);
CREATE INDEX idx_review_decision_submission ON kyc_review_decision(submission_id, decided_at DESC);
