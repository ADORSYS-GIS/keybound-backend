UPDATE sm_instance
SET
  kind = 'KYC_PHONE_OTP',
  idempotency_key = regexp_replace(
    idempotency_key,
    '^KYC_EMAIL_MAGIC:EMAIL_MAGIC:',
    'KYC_PHONE_OTP:EMAIL_MAGIC:'
  )
WHERE kind = 'KYC_EMAIL_MAGIC'
  AND context ->> 'flow' = 'EMAIL_MAGIC';
