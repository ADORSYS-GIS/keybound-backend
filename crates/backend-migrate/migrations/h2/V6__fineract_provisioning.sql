CREATE TABLE fineract_provisioning (
  id varchar(40) PRIMARY KEY,
  kyc_case_id varchar(40) NOT NULL,
  submission_id varchar(40) NOT NULL,
  status varchar(16) NOT NULL,
  fineract_customer_id varchar(128),
  error_code varchar(64),
  error_message clob,
  attempt_no int NOT NULL DEFAULT 1,
  created_at timestamp with time zone NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at timestamp with time zone NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT fk_prov_case FOREIGN KEY (kyc_case_id) REFERENCES kyc_case(id),
  CONSTRAINT fk_prov_submission FOREIGN KEY (submission_id) REFERENCES kyc_submission(id)
);
