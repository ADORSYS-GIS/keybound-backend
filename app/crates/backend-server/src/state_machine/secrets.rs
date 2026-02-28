use backend_core::Error;

pub fn hash_secret(secret: &str) -> Result<String, Error> {
    use argon2::password_hash::rand_core::OsRng;
    use argon2::password_hash::{PasswordHasher, SaltString};

    let salt = SaltString::generate(&mut OsRng);
    argon2::Argon2::default()
        .hash_password(secret.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|err| Error::internal("ARGON2_HASH_FAILED", err.to_string()))
}

pub fn verify_secret(plain: &str, hash: &str) -> Result<bool, Error> {
    use argon2::password_hash::{PasswordHash, PasswordVerifier};

    let parsed = PasswordHash::new(hash)
        .map_err(|err| Error::internal("ARGON2_HASH_INVALID", err.to_string()))?;

    Ok(argon2::Argon2::default()
        .verify_password(plain.as_bytes(), &parsed)
        .is_ok())
}
