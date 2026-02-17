-- Consolidate KYC data into kyc_submission and remove redundant kyc_profiles table

ALTER TABLE kyc_submission
  ADD COLUMN first_name text,
  ADD COLUMN last_name text,
  ADD COLUMN email text,
  ADD COLUMN phone_number text,
  ADD COLUMN date_of_birth text,
  ADD COLUMN nationality text,
  ADD COLUMN rejection_reason text,
  ADD COLUMN review_notes text;
