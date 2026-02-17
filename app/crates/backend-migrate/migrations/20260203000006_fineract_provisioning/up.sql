CREATE TYPE provisioning_status AS ENUM (
  'STARTED',
  'SUCCEEDED',
  'FAILED'
);

CREATE TABLE fineract_provisioning (
  id varchar(40) PRIMARY KEY,
  kyc_case_id varchar(40) NOT NULL REFERENCES kyc_case(id),
  submission_id varchar(40) NOT NULL REFERENCES kyc_submission(id),
  status provisioning_status NOT NULL,
  fineract_customer_id varchar(128),
  error_code varchar(64),
  error_message text,
  attempt_no int NOT NULL DEFAULT 1,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now()
);
