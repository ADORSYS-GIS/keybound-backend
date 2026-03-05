ALTER TABLE app_user ADD COLUMN first_name text;
ALTER TABLE app_user ADD COLUMN last_name text;
ALTER TABLE app_user DROP COLUMN IF EXISTS full_name;
