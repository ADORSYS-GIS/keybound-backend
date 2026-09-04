//! JWT claims structures for authentication.
//!
//! Defines the standard claims extracted from JWT tokens issued by the OAuth2 provider.

use serde::Deserialize;

#[derive(Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum AudienceClaim {
    One(String),
    Many(Vec<String>),
}

impl AudienceClaim {
    pub fn contains(&self, expected: &str) -> bool {
        match self {
            Self::One(value) => value == expected,
            Self::Many(values) => values.iter().any(|value| value == expected),
        }
    }

    pub fn values(&self) -> Vec<String> {
        match self {
            Self::One(value) => vec![value.clone()],
            Self::Many(values) => values.clone(),
        }
    }
}

/// JWT token claims from the OAuth2/OIDC provider (typically Keycloak).
#[derive(Deserialize, Clone, Debug)]
pub struct Claims {
    /// Subject identifier (user ID)
    pub sub: String,
    #[serde(default)]
    pub azp: Option<String>,
    #[serde(default)]
    pub aud: Option<AudienceClaim>,
    #[serde(default)]
    pub scope: Option<String>,
    /// Full name of the user (optional)
    #[serde(default)]
    pub name: Option<String>,
    /// Issuer URL
    pub iss: String,
    /// Expiration timestamp (Unix epoch seconds)
    pub exp: usize,
    /// Preferred username (often used as fallback for name)
    #[serde(default)]
    pub preferred_username: Option<String>,
}

impl Claims {
    /// Returns the user's display name, preferring 'name' over 'preferred_username'.
    pub fn get_name(&self) -> Option<String> {
        self.name
            .clone()
            .or_else(|| self.preferred_username.clone())
    }
}
