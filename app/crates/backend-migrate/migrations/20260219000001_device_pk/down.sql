BEGIN;

CREATE TABLE device_old (
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

INSERT INTO device_old (device_id, user_id, jkt, public_jwk, status, label, created_at, last_seen_at)
SELECT device_id, user_id, jkt, public_jwk, status, label, created_at, last_seen_at
FROM device;

DROP TABLE device;

ALTER TABLE device_old RENAME TO device;

COMMIT;
