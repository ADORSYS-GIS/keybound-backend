INSERT INTO users (
  user_id, realm, username, first_name, last_name, email, enabled, email_verified, attributes
)
VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
RETURNING
  user_id, realm, username, first_name, last_name, email, enabled, email_verified,
  attributes, created_at, updated_at