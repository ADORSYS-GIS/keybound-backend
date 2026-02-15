INSERT INTO approvals (
  request_id, realm, client_id, user_id, device_id, jkt, public_jwk,
  platform, model, app_version, reason, expires_at, context, idempotency_key
)
VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)
RETURNING request_id, status::text as status, expires_at