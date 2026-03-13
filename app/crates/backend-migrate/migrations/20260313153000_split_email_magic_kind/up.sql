UPDATE sm_instance
SET
  kind = 'KYC_EMAIL_MAGIC',
  idempotency_key = regexp_replace(
    idempotency_key,
    '^KYC_PHONE_OTP:EMAIL_MAGIC:',
    'KYC_EMAIL_MAGIC:EMAIL_MAGIC:'
  )
WHERE kind = 'KYC_PHONE_OTP'
  AND context ->> 'flow' = 'EMAIL_MAGIC';
