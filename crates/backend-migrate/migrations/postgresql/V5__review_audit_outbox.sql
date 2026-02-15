CREATE TYPE kyc_review_action_type AS ENUM (
  'APPROVE',
  'REJECT',
  'REQUEST_INFO',
  'ASSIGN'
);

CREATE TYPE actor_type AS ENUM (
  'USER',
  'STAFF',
  'SYSTEM'
);

CREATE TABLE kyc_review_action (
  id varchar(40) PRIMARY KEY,
  submission_id varchar(40) NOT NULL REFERENCES kyc_submission(id) ON DELETE CASCADE,
  action kyc_review_action_type NOT NULL,
  staff_user_id varchar(40),
  payload jsonb NOT NULL DEFAULT '{}'::jsonb,
  created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE kyc_audit_log (
  id varchar(40) PRIMARY KEY,
  submission_id varchar(40) NOT NULL REFERENCES kyc_submission(id) ON DELETE CASCADE,
  event_type varchar(128) NOT NULL,
  actor_type actor_type NOT NULL,
  actor_id varchar(40),
  metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
  created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE outbox_event (
  id varchar(40) PRIMARY KEY,
  aggregate_type varchar(64) NOT NULL,
  aggregate_id varchar(40) NOT NULL,
  event_type varchar(128) NOT NULL,
  payload jsonb NOT NULL,
  status varchar(16) NOT NULL DEFAULT 'NEW',
  attempts int NOT NULL DEFAULT 0,
  next_retry_at timestamptz,
  created_at timestamptz NOT NULL DEFAULT now()
);
