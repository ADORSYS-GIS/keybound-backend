-- Account-recovery case aggregate.
--
-- The recovery case is the user-storage-owned aggregate that ties together a
-- recovery flow session, the matched existing account, OTP verification,
-- admin review, and final device binding. Previously the case was only an
-- opaque string in the two idempotency tables; this table is the authoritative
-- source of truth for case state and projection.

CREATE TABLE recovery_case (
  id TEXT PRIMARY KEY,
  human_id TEXT UNIQUE NOT NULL,
  session_id TEXT REFERENCES flow_session(id) ON DELETE SET NULL,
  device_id TEXT,
  jkt TEXT,
  device_public_jwk JSONB,
  requested_phone_hash TEXT NOT NULL,
  requested_phone_masked TEXT NOT NULL,
  reason TEXT,
  status TEXT NOT NULL DEFAULT 'CREATED',
  phone_relation TEXT,
  matched_user_id TEXT,
  otp_hash TEXT,
  otp_expires_at TIMESTAMPTZ,
  otp_attempts INT NOT NULL DEFAULT 0,
  otp_resend_at TIMESTAMPTZ,
  review_decision TEXT,
  review_reason TEXT,
  review_checklist JSONB,
  review_expected_version BIGINT,
  reviewer_id TEXT,
  decided_at TIMESTAMPTZ,
  approval_revision BIGINT,
  old_devices JSONB NOT NULL DEFAULT '[]'::jsonb,
  risk_flags JSONB NOT NULL DEFAULT '{}'::jsonb,
  expires_at TIMESTAMPTZ,
  review_expires_at TIMESTAMPTZ,
  approved_expires_at TIMESTAMPTZ,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  version BIGINT NOT NULL DEFAULT 1
);

CREATE INDEX idx_recovery_case_session ON recovery_case(session_id);
CREATE INDEX idx_recovery_case_status ON recovery_case(status);
CREATE INDEX idx_recovery_case_phone_hash ON recovery_case(requested_phone_hash);
CREATE INDEX idx_recovery_case_matched_user ON recovery_case(matched_user_id);
