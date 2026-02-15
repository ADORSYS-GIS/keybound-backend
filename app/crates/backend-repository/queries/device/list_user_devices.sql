SELECT
  id,
  realm,
  client_id,
  user_id,
  user_hint,
  device_id,
  jkt,
  status::text as status,
  public_jwk,
  attributes,
  proof,
  label,
  created_at,
  last_seen_at
FROM devices
WHERE user_id = $1
  AND ($2 OR status = 'ACTIVE')
ORDER BY created_at DESC