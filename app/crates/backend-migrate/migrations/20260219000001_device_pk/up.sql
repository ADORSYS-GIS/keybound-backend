BEGIN;

ALTER TABLE device RENAME TO device_old;

CREATE TABLE device (
    device_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    jkt TEXT NOT NULL,
    public_jwk TEXT NOT NULL,
    status TEXT NOT NULL,
    label TEXT,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL,
    last_seen_at TIMESTAMP WITH TIME ZONE,
    PRIMARY KEY (device_id, public_jwk),
    CONSTRAINT fk_device_user FOREIGN KEY (user_id) REFERENCES app_user (user_id)
);

CREATE UNIQUE INDEX device_device_id_unique ON device (device_id);
CREATE UNIQUE INDEX device_public_jwk_unique ON device (public_jwk);

INSERT INTO device (device_id, user_id, jkt, public_jwk, status, label, created_at, last_seen_at)
SELECT device_id, user_id, jkt, public_jwk, status, label, created_at, last_seen_at
FROM device_old;

DROP TABLE device_old;

COMMIT;
