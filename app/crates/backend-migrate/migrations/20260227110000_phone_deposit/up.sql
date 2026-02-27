CREATE TABLE phone_deposit (
  deposit_id text PRIMARY KEY,
  user_id text NOT NULL REFERENCES app_user(user_id),
  amount double precision NOT NULL,
  currency text NOT NULL,
  reason text,
  reference text,
  status text NOT NULL,
  staff_id text NOT NULL,
  staff_full_name text NOT NULL,
  staff_phone_number text NOT NULL,
  expires_at timestamptz,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT phone_deposit_amount_check CHECK (amount >= 0.01),
  CONSTRAINT phone_deposit_status_check CHECK (status IN ('CREATED','CONTACT_PROVIDED','PAID','CANCELLED','EXPIRED'))
);

CREATE INDEX phone_deposit_user_created_idx ON phone_deposit(user_id, created_at DESC);
CREATE INDEX phone_deposit_status_created_idx ON phone_deposit(status, created_at DESC);
