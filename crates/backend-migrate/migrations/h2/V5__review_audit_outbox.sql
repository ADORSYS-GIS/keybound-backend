CREATE TABLE kyc_review_action (
  id varchar(40) PRIMARY KEY,
  submission_id varchar(40) NOT NULL,
  action varchar(32) NOT NULL,
  staff_user_id varchar(40),
  payload clob NOT NULL DEFAULT '{}',
  created_at timestamp with time zone NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT fk_review_submission FOREIGN KEY (submission_id) REFERENCES kyc_submission(id) ON DELETE CASCADE
);

CREATE TABLE kyc_audit_log (
  id varchar(40) PRIMARY KEY,
  submission_id varchar(40) NOT NULL,
  event_type varchar(128) NOT NULL,
  actor_type varchar(16) NOT NULL,
  actor_id varchar(40),
  metadata clob NOT NULL DEFAULT '{}',
  created_at timestamp with time zone NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT fk_audit_submission FOREIGN KEY (submission_id) REFERENCES kyc_submission(id) ON DELETE CASCADE
);

CREATE TABLE outbox_event (
  id varchar(40) PRIMARY KEY,
  aggregate_type varchar(64) NOT NULL,
  aggregate_id varchar(40) NOT NULL,
  event_type varchar(128) NOT NULL,
  payload clob NOT NULL,
  status varchar(16) NOT NULL DEFAULT 'NEW',
  attempts int NOT NULL DEFAULT 0,
  next_retry_at timestamp with time zone,
  created_at timestamp with time zone NOT NULL DEFAULT CURRENT_TIMESTAMP
);
