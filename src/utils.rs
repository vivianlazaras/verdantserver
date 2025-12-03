use crate::config::VerdantConfig;
use rocket_oidc::CoreClaims;
use serde_derive::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthClaims {
    pub sub: String,
    pub exp: i64,
    pub iat: i64,
    pub aud: String,
    pub iss: String,
}

impl AuthClaims {
    pub fn new(subject: &str, audience: &str, issuer: &str) -> Self {
        // Build JWT-like claims as a serde_json::Value (OidcSigner accepts any Serialize)
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as usize;

        AuthClaims {
            sub: subject.to_string(), // to be filled in later
            exp: (now + 3600) as i64, // 1 hour expiration
            iat: now as i64,
            iss: issuer.to_string(),
            aud: audience.to_string(),
        }
    }

    pub(crate) fn issue_jwt(config: &VerdantConfig, subject_str: &str) -> Option<String> {
        let claims = AuthClaims::new(&subject_str, "verdant", &config.issuer_url);

        // Sign using the OidcSigner from the config (sign takes (claims, Duration))
        let token = match config.signer.sign(&claims) {
            Ok(t) => t,
            Err(e) => {
                println!("error: {}", e);
                return None;
            }
        };

        Some(token)
    }
}

impl CoreClaims for AuthClaims {
    fn subject(&self) -> &str {
        self.sub.as_ref()
    }

    fn audience(&self) -> &str {
        self.aud.as_ref()
    }

    fn issuer(&self) -> &str {
        self.iss.as_ref()
    }

    fn issued_at(&self) -> i64 {
        self.iat
    }

    fn expiration(&self) -> i64 {
        self.exp
    }
}

pub type Guard = rocket_oidc::auth::AuthGuard<AuthClaims>;
pub type KeyGuard = rocket_oidc::auth::ApiKeyGuard<AuthClaims>;

pub fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs() as i64
}
