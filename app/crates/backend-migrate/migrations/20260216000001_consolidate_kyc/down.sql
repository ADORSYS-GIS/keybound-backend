ALTER TABLE kyc_submission
  DROP COLUMN IF EXISTS first_name,
  DROP COLUMN IF EXISTS last_name,
  DROP COLUMN IF EXISTS email,
  DROP COLUMN IF EXISTS phone_number,
  DROP COLUMN IF EXISTS date_of_birth,
  DROP COLUMN IF EXISTS nationality,
  DROP COLUMN IF EXISTS rejection_reason,
  DROP COLUMN IF EXISTS review_notes;
