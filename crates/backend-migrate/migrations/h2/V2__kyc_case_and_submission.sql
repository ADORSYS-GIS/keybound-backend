CREATE TABLE kyc_case (
  id varchar(40) PRIMARY KEY,
  user_id varchar(40) NOT NULL,
  current_tier int NOT NULL DEFAULT 0,
  case_status varchar(32) NOT NULL DEFAULT 'OPEN',
  active_submission_id varchar(40),
  created_at timestamp with time zone NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at timestamp with time zone NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT fk_kyc_case_user FOREIGN KEY (user_id) REFERENCES app_user(id),
  CONSTRAINT uq_kyc_case_user UNIQUE(user_id)
);

CREATE TABLE kyc_submission (
  id varchar(40) PRIMARY KEY,
  kyc_case_id varchar(40) NOT NULL,
  version int NOT NULL,
  status varchar(64) NOT NULL DEFAULT 'DRAFT',
  requested_tier int NOT NULL DEFAULT 1,
  decided_tier int,
  submitted_at timestamp with time zone,
  decided_at timestamp with time zone,
  decided_by varchar(40),
  provisioning_status varchar(32) NOT NULL DEFAULT 'NONE',
  created_at timestamp with time zone NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at timestamp with time zone NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT fk_kyc_submission_case FOREIGN KEY (kyc_case_id) REFERENCES kyc_case(id) ON DELETE CASCADE,
  CONSTRAINT uq_kyc_submission_case_version UNIQUE(kyc_case_id, version)
);

ALTER TABLE kyc_case
  ADD CONSTRAINT fk_active_submission
  FOREIGN KEY (active_submission_id)
  REFERENCES kyc_submission(id);
