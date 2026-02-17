CREATE TABLE device (
    device_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    jkt TEXT NOT NULL,
    public_jwk TEXT NOT NULL,
    status TEXT NOT NULL,
    label TEXT,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL,
    last_seen_at TIMESTAMP WITH TIME ZONE,
    PRIMARY KEY (device_id),
    CONSTRAINT fk_device_user FOREIGN KEY (user_id) REFERENCES app_user (user_id)
);

CREATE TABLE approval (
    request_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    new_device_id TEXT NOT NULL,
    new_device_jkt TEXT NOT NULL,
    new_device_public_jwk TEXT NOT NULL,
    new_device_platform TEXT,
    new_device_model TEXT,
    new_device_app_version TEXT,
    status TEXT NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL,
    expires_at TIMESTAMP WITH TIME ZONE NOT NULL,
    decided_at TIMESTAMP WITH TIME ZONE,
    decided_by_device_id TEXT,
    message TEXT,
    PRIMARY KEY (request_id),
    CONSTRAINT fk_approval_user FOREIGN KEY (user_id) REFERENCES app_user (user_id)
);
