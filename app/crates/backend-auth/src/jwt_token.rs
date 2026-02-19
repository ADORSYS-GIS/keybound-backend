use crate::claims::Claims;
use crate::oidc_state::OidcState;
use backend_core::{Error, Result};
use jsonwebtoken::{DecodingKey, Validation, decode};
use tracing::instrument;

#[derive(Debug, Clone)]
pub struct JwtToken {
    pub claims: Claims,
}

impl JwtToken {
    pub fn new(claims: Claims) -> JwtToken {
        JwtToken { claims }
    }

    pub fn user_id(&self) -> &str {
        &self.claims.sub
    }

    #[instrument(skip(oidc_state))]
    pub async fn verify(
        token: &str,
        oidc_state: &OidcState,
    ) -> Result<Self> {
        let jwks = oidc_state.get_jwks().await?;

        let header = jsonwebtoken::decode_header(token)
            .map_err(|e| Error::unauthorized(format!("Invalid token header: {e}")))?;

        let kid = header.kid.ok_or_else(|| Error::unauthorized("Missing kid in token header"))?;

        let jwk = jwks
            .find(&kid)
            .ok_or_else(|| Error::unauthorized(format!("Key ID {kid} not found in JWKS")))?;

        let decoding_key = DecodingKey::from_jwk(jwk)
            .map_err(|e| Error::unauthorized(format!("Invalid JWK: {e}")))?;

        let mut validation = Validation::new(header.alg);
        if let Some(audiences) = &oidc_state.audiences {
            validation.set_audience(audiences);
        } else {
            validation.validate_aud = false;
        }

        let token_data = decode::<Claims>(token, &decoding_key, &validation)
            .map_err(|e| Error::unauthorized(format!("Token validation failed: {e}")))?;

        Ok(JwtToken::new(token_data.claims))
    }
}
