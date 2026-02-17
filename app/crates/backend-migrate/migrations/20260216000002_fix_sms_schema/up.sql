CREATE TABLE sms_messages (
    id VARCHAR(40) NOT NULL PRIMARY KEY,
    realm VARCHAR(255) NOT NULL,
    client_id VARCHAR(255) NOT NULL,
    user_id VARCHAR(40),
    phone_number VARCHAR(64) NOT NULL,
    hash VARCHAR(64) NOT NULL,
    otp_sha256 BYTEA NOT NULL,
    ttl_seconds INTEGER NOT NULL,
    status VARCHAR(32) NOT NULL DEFAULT 'PENDING',
    attempt_count INTEGER NOT NULL DEFAULT 0,
    max_attempts INTEGER NOT NULL DEFAULT 3,
    next_retry_at TIMESTAMP WITH TIME ZONE,
    last_error TEXT,
    sns_message_id VARCHAR(255),
    session_id VARCHAR(255),
    trace_id VARCHAR(255),
    metadata JSONB,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    sent_at TIMESTAMP WITH TIME ZONE,
    confirmed_at TIMESTAMP WITH TIME ZONE
);

CREATE INDEX idx_sms_messages_hash ON sms_messages(hash);
CREATE INDEX idx_sms_messages_status_retry ON sms_messages(status, next_retry_at);
