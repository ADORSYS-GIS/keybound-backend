INSERT INTO sms_messages (
  id, realm, client_id, user_id, phone_number, hash, otp_sha256, ttl_seconds,
  max_attempts, next_retry_at, metadata
)
VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,now(),$10)