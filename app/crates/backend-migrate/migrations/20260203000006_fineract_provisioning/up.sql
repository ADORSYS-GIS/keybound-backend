CREATE TYPE provisioning_status AS ENUM (
  'STARTED',
  'SUCCEEDED',
  'FAILED'
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
