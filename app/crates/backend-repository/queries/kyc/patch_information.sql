UPDATE kyc_profiles
SET
  first_name = COALESCE($2, first_name),
  last_name = COALESCE($3, last_name),
  email = COALESCE($4, email),
  phone_number = COALESCE($5, phone_number),
  date_of_birth = COALESCE($6, date_of_birth),
  nationality = COALESCE($7, nationality),
  updated_at = now()
WHERE external_id = $1
RETURNING
  external_id,
  first_name,
  last_name,
  email,
  phone_number,
  date_of_birth,
  nationality,
  kyc_tier,
  kyc_status::text as kyc_status,
  submitted_at,
  reviewed_at,
  reviewed_by,
  rejection_reason,
  review_notes,
  created_at,
  updated_at