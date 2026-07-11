use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString, IntoStaticStr};

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Default,
    Display,
    EnumString,
    IntoStaticStr,
    Serialize,
    Deserialize,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum SecretType {
    /// Opaque API-key/PAT-style token value.
    #[default]
    Token,
    /// JSON-encoded OAuth credential. Refreshable; never projected into env.
    Oauth,
    /// Path-shaped secret materialized to the filesystem.
    File,
}

/// JSON shape stored when [`SecretType::Oauth`] is used.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OAuthCredential {
    pub tokens:     OAuthTokens,
    pub config:     OAuthConfig,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
}

impl OAuthCredential {
    #[must_use]
    pub fn needs_refresh(&self) -> bool {
        self.tokens.expires_at <= Utc::now() + Duration::minutes(5)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OAuthTokens {
    pub access_token:  String,
    pub refresh_token: Option<String>,
    pub expires_at:    DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OAuthConfig {
    pub auth_url:     String,
    pub token_url:    String,
    pub client_id:    String,
    pub scopes:       Vec<String>,
    pub redirect_uri: Option<String>,
    pub use_pkce:     bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretMetadata {
    pub name:        String,
    #[serde(rename = "type")]
    pub secret_type: SecretType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub created_at:  DateTime<Utc>,
    pub updated_at:  DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_type_serializes_to_snake_case() {
        assert_eq!(
            serde_json::to_string(&SecretType::Token).unwrap(),
            "\"token\""
        );
        assert_eq!(
            serde_json::to_string(&SecretType::Oauth).unwrap(),
            "\"oauth\""
        );
        assert_eq!(
            serde_json::to_string(&SecretType::File).unwrap(),
            "\"file\""
        );
    }

    #[test]
    fn secret_type_default_is_token() {
        assert_eq!(SecretType::default(), SecretType::Token);
    }
}
