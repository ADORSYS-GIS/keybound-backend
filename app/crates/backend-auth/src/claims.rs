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

/// Keycloak `realm_access` claim (realm roles assigned to the principal).
#[derive(Deserialize, Clone, Debug, Default)]
pub struct RealmAccess {
    #[serde(default)]
    pub roles: Vec<String>,
}

impl RealmAccess {
    pub fn contains_role(&self, role: &str) -> bool {
        self.roles.iter().any(|candidate| candidate == role)
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
    /// Keycloak realm roles (e.g. staff/recovery-admin).
    #[serde(default)]
    pub realm_access: Option<RealmAccess>,
    /// Keycloak group memberships (alternate source of staff authorization).
    #[serde(default)]
    pub groups: Option<Vec<String>>,
}

impl Claims {
    /// Returns the user's display name, preferring 'name' over 'preferred_username'.
    pub fn get_name(&self) -> Option<String> {
        self.name
            .clone()
            .or_else(|| self.preferred_username.clone())
    }

    /// Returns true when the principal carries `role` as a Keycloak realm role
    /// or as a group membership.
    pub fn has_realm_role(&self, role: &str) -> bool {
        self.realm_access
            .as_ref()
            .map(|access| access.contains_role(role))
            .unwrap_or(false)
            || self
                .groups
                .as_ref()
                .map(|groups| groups.iter().any(|group| group == role))
                .unwrap_or(false)
    }
}
