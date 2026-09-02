CREATE TABLE recovery_idempotency (
  idempotency_key text PRIMARY KEY,
  recovery_case_id text NOT NULL,
  request_hash text NOT NULL,
  bound_user_id text NOT NULL,
  device_id text NOT NULL,
  binding_operation_id text NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX recovery_idempotency_binding_op_idx ON recovery_idempotency(binding_operation_id);
