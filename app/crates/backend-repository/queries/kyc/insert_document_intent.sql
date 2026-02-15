INSERT INTO kyc_documents (
  id, external_id, document_type, file_name, mime_type, content_length,
  s3_bucket, s3_key, presigned_expires_at
)
VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
RETURNING
  id,
  external_id,
  document_type,
  status::text as status,
  uploaded_at,
  rejection_reason,
  file_name,
  mime_type,
  content_length,
  s3_bucket,
  s3_key,
  presigned_expires_at,
  created_at,
  updated_at