UPDATE users
SET
  realm = $2,
  username = $3,
  first_name = $4,
  last_name = $5,
  email = $6,
  enabled = $7,
  email_verified = $8,
  attributes = $9,
  updated_at = now()
WHERE user_id = $1
RETURNING
  user_id, realm, username, first_name, last_name, email, enabled, email_verified,
  attributes, created_at, updated_at