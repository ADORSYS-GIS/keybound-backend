CREATE TABLE app_user (
  id varchar(40) PRIMARY KEY,
  email varchar(320),
  email_verified boolean NOT NULL DEFAULT false,
  phone_number varchar(64),
  fineract_customer_id varchar(128),
  disabled boolean NOT NULL DEFAULT false,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX idx_user_phone ON app_user(phone_number);
CREATE INDEX idx_user_email ON app_user(email);
CREATE INDEX idx_user_fineract ON app_user(fineract_customer_id);
