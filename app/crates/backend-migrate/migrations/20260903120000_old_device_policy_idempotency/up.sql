CREATE TABLE old_device_policy_idempotency (
  idempotency_key text PRIMARY KEY,
  recovery_case_id text NOT NULL,
  request_hash text NOT NULL,
  target_user_id text NOT NULL,
  policy text NOT NULL,
  affected_device_ids jsonb NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now()
);

-- A recovery case's old-device policy is applied exactly once. This prevents a
-- retry with a different idempotency key from re-applying (or re-reporting) the
-- policy for the same case.
CREATE UNIQUE INDEX old_device_policy_case_uq ON old_device_policy_idempotency(recovery_case_id);
