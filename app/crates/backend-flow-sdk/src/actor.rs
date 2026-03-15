use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Actor {
    System,
    Admin,
    EndUser,
}

impl fmt::Display for Actor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Actor::System => write!(f, "SYSTEM"),
            Actor::Admin => write!(f, "ADMIN"),
            Actor::EndUser => write!(f, "END_USER"),
        }
    }
}
