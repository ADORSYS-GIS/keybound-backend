ALTER TABLE kyc_case ADD COLUMN current_tier int NOT NULL DEFAULT 0;
ALTER TABLE kyc_submission ADD COLUMN requested_tier int NOT NULL DEFAULT 1;
ALTER TABLE kyc_submission ADD COLUMN decided_tier int;
