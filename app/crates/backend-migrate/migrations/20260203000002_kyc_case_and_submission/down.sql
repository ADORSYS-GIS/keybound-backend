ALTER TABLE kyc_case DROP CONSTRAINT IF EXISTS fk_active_submission;
DROP TABLE IF EXISTS kyc_submission;
DROP TABLE IF EXISTS kyc_case;
DROP TYPE IF EXISTS kyc_provisioning_status;
DROP TYPE IF EXISTS kyc_submission_status;
DROP TYPE IF EXISTS kyc_case_status;
