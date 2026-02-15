SELECT
  user_id, realm, username, first_name, last_name, email, enabled, email_verified,
  attributes, created_at, updated_at
FROM users
WHERE realm = $1 AND username = $2