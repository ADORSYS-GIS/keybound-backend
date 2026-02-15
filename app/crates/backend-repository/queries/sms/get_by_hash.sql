SELECT
  id,
  realm,
  client_id,
  user_id,
  phone_number,
  hash,
  otp_sha256,
  ttl_seconds,
  status::text as status,
  attempt_count,
  max_attempts,
  next_retry_at,
  last_error,
  sns_message_id,
  session_id,
  trace_id,
  metadata,
  created_at,
  sent_at,
  confirmed_at
FROM sms_messages
WHERE hash = $1