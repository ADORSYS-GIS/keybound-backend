CREATE INDEX idx_user_phone ON app_user(phone_number);
CREATE INDEX idx_kyc_case_active_submission ON kyc_case(active_submission_id);
CREATE INDEX idx_kyc_submission_status ON kyc_submission(status);
