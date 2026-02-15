INSERT INTO devices (
  id, realm, client_id, user_id, user_hint, device_id, jkt, public_jwk, attributes, proof
)
VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
RETURNING id