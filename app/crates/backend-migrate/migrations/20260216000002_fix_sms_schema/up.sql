CREATE TABLE sms_messages (
    id TEXT NOT NULL PRIMARY KEY,
    realm TEXT NOT NULL,
    client_id TEXT NOT NULL,
    user_id TEXT,
    phone_number TEXT NOT NULL,
    hash TEXT NOT NULL,
    otp_sha256 BYTEA NOT NULL,
    ttl_seconds INTEGER NOT NULL,
    status TEXT NOT NULL DEFAULT 'PENDING',
    attempt_count INTEGER NOT NULL DEFAULT 0,
    max_attempts INTEGER NOT NULL DEFAULT 3,
    next_retry_at TIMESTAMP WITH TIME ZONE,
    last_error TEXT,
    sns_message_id TEXT,
    session_id TEXT,
    trace_id TEXT,
    metadata JSONB,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    sent_at TIMESTAMP WITH TIME ZONE,
    confirmed_at TIMESTAMP WITH TIME ZONE
);

CREATE INDEX idx_sms_messages_hash ON sms_messages(hash);
CREATE INDEX idx_sms_messages_status_retry ON sms_messages(status, next_retry_at);
