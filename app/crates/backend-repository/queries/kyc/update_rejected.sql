UPDATE kyc_profiles
SET
  kyc_status = 'REJECTED',
  reviewed_at = now(),
  reviewed_by = 'staff',
  rejection_reason = $2,
  review_notes = $3,
  updated_at = now()
WHERE external_id = $1