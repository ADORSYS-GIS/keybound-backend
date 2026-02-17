CREATE TABLE device (
    device_id VARCHAR(40) NOT NULL,
    user_id VARCHAR(40) NOT NULL,
    jkt VARCHAR(255) NOT NULL,
    public_jwk TEXT NOT NULL,
    status VARCHAR(255) NOT NULL,
    label VARCHAR(255),
    created_at TIMESTAMP WITH TIME ZONE NOT NULL,
    last_seen_at TIMESTAMP WITH TIME ZONE,
    PRIMARY KEY (device_id),
    CONSTRAINT fk_device_user FOREIGN KEY (user_id) REFERENCES app_user (user_id)
);

CREATE TABLE approval (
    request_id VARCHAR(40) NOT NULL,
    user_id VARCHAR(40) NOT NULL,
    new_device_id VARCHAR(40) NOT NULL,
    new_device_jkt VARCHAR(255) NOT NULL,
    new_device_public_jwk TEXT NOT NULL,
    new_device_platform VARCHAR(64),
    new_device_model VARCHAR(128),
    new_device_app_version VARCHAR(64),
    status VARCHAR(255) NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL,
    expires_at TIMESTAMP WITH TIME ZONE NOT NULL,
    decided_at TIMESTAMP WITH TIME ZONE,
    decided_by_device_id VARCHAR(40),
    message VARCHAR(512),
    PRIMARY KEY (request_id),
    CONSTRAINT fk_approval_user FOREIGN KEY (user_id) REFERENCES app_user (user_id)
);
