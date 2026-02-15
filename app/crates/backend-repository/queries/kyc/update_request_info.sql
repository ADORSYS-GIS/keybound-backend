UPDATE kyc_profiles
SET
  kyc_status = 'NEEDS_INFO',
  reviewed_at = now(),
  reviewed_by = 'staff',
  review_notes = $2,
  updated_at = now()
WHERE external_id = $1