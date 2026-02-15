UPDATE sms_messages
SET
  status = 'SENT',
  attempt_count = attempt_count + 1,
  sns_message_id = $2,
  sent_at = now(),
  last_error = NULL,
  next_retry_at = NULL
WHERE id = $1