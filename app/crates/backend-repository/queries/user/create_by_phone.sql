INSERT INTO users (user_id, realm, username, enabled, email_verified, attributes)
VALUES ($1,$2,$3,TRUE,FALSE,$4)
RETURNING
  user_id, realm, username, first_name, last_name, email, enabled, email_verified,
  attributes, created_at, updated_at