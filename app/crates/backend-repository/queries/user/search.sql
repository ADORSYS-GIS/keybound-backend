SELECT
  user_id, realm, username, first_name, last_name, email, enabled, email_verified,
  attributes, created_at, updated_at
FROM users
WHERE realm = $1
  AND ($2::text IS NULL OR (
    username ILIKE ('%' || $2 || '%') OR
    email ILIKE ('%' || $2 || '%') OR
    first_name ILIKE ('%' || $2 || '%') OR
    last_name ILIKE ('%' || $2 || '%')
  ))
  AND ($3::text IS NULL OR username = $3)
  AND ($4::text IS NULL OR email = $4)
  AND ($5::boolean IS NULL OR enabled = $5)
  AND ($6::boolean IS NULL OR email_verified = $6)
ORDER BY created_at DESC
LIMIT $7
OFFSET $8