SELECT
  request_id,
  realm,
  client_id,
  user_id,
  device_id,
  jkt,
  public_jwk,
  platform,
  model,
  app_version,
  reason,
  expires_at,
  context,
  idempotency_key,
  status::text as status,
  created_at,
  decided_at,
  decided_by_device_id,
  message
FROM approvals
WHERE user_id = $1
  AND ($2::text[] IS NULL OR status::text = ANY($2))
ORDER BY created_at DESC