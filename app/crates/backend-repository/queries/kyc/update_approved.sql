UPDATE kyc_profiles
SET
  kyc_status = 'APPROVED',
  kyc_tier = $2,
  reviewed_at = now(),
  reviewed_by = 'staff',
  review_notes = $3,
  updated_at = now()
WHERE external_id = $1