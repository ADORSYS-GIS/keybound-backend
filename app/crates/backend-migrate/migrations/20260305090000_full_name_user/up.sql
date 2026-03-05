ALTER TABLE app_user
  ADD COLUMN full_name text;

UPDATE app_user
SET full_name = NULLIF(
  trim(concat_ws(' ',
    coalesce(nullif(first_name, ''), ''),
    coalesce(nullif(last_name, ''), '')
  )),
  ''
)
WHERE full_name IS NULL;

ALTER TABLE app_user DROP COLUMN IF EXISTS first_name;
ALTER TABLE app_user DROP COLUMN IF EXISTS last_name;
