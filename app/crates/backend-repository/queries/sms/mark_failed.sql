UPDATE sms_messages
SET
  status = $2::sms_status,
  attempt_count = attempt_count + 1,
  last_error = $3,
  next_retry_at = $4
WHERE id = $1