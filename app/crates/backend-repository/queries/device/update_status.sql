UPDATE devices
SET status = $2::device_status
WHERE id = $1
RETURNING
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