use crate::backend::Guard;
use livekit::prelude::{Room, RoomOptions};
use livekit_api::access_token;
use livekit_api::services::ServiceError;
use livekit_api::services::room::RoomClient;
use reqwest::Client;
use rocket::http::Status;
use rocket::{Route, State, get, post, response::status, routes, serde::json::Json};
use rocket_dyn_templates::Template;
use rocket_oidc::CoreClaims;
use rocket_oidc::auth::AuthGuard;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};

/// Lightweight configuration for communicating with LiveKit
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LivekitConfig {
    pub base_url: String, // e.g. "https://livekit.example.com"
    pub api_key: String,
    pub api_secret: String, // used to sign access tokens (HMAC)
}

#[derive(Serialize)]
pub struct TokenResponse {
    pub token: String,
    pub room: String,
}

/// Minimal room representation returned by LiveKit /rooms endpoint
#[derive(Deserialize, Serialize, Debug)]
pub struct RoomInfo {
    pub name: String,
    // add other fields you care about from the LiveKit API response
}

impl LivekitConfig {
    pub fn client(&self) -> Client {
        Client::new()
    }

    pub fn room_client(&self) -> RoomClient {
        RoomClient::with_api_key(&self.base_url, &self.api_key, &self.api_secret)
    }

    pub fn from_env() -> Self {
        let base_url = std::env::var("LIVEKIT_URL")
            .or_else(|_| std::env::var("LIVEKIT_BASE_URL"))
            .unwrap_or_else(|_| {
                panic!("environment variable LIVEKIT_URL or LIVEKIT_BASE_URL must be set")
            });
        let api_key = std::env::var("LIVEKIT_API_KEY")
            .unwrap_or_else(|_| panic!("environment variable LIVEKIT_API_KEY must be set"));
        let api_secret = std::env::var("LIVEKIT_API_SECRET")
            .unwrap_or_else(|_| panic!("environment variable LIVEKIT_API_SECRET must be set"));

        LivekitConfig {
            base_url,
            api_key,
            api_secret,
        }
    }

    pub fn new(base_url: &str, api_key: &str, api_secret: &str) -> Self {
        LivekitConfig {
            base_url: base_url.to_string(),
            api_key: api_key.to_string(),
            api_secret: api_secret.to_string(),
        }
    }
}

async fn get_access_token(
    cfg: &LivekitConfig,
    identity: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let token = access_token::AccessToken::with_api_key(&cfg.api_key, &cfg.api_secret)
        .with_identity(identity)
        .with_name(identity)
        .with_grants(access_token::VideoGrants {
            room_join: true,
            room: identity.to_string(),
            ..Default::default()
        })
        .to_jwt()?;

    let (room, mut rx) = Room::connect(&cfg.base_url, &token, RoomOptions::default()).await?;

    Ok(token)
}

/// GET /token
/// Creates a default room (if missing) and returns an access token for the participant.
/// This route is intentionally not protected (e.g., for application login flow you might protect it).
#[get("/token")]
pub async fn token_route(
    guard: Guard,
    cfg: &State<LivekitConfig>,
) -> Result<Json<TokenResponse>, status::Custom<String>> {
    let identity = guard.claims.subject().to_string();
    let token = get_access_token(&cfg, &identity)
        .await
        .map_err(|e| status::Custom(Status::InternalServerError, format!("{}", e)))?;

    Ok(Json(TokenResponse {
        token,
        room: identity,
    }))
}

/// GET /rooms
/// Protected by rocket_oidc::AuthGuard — only authenticated requests allowed.
/// Returns a list of rooms from the LiveKit server.
#[get("/rooms")]
pub async fn list_rooms_route(
    guard: Guard,
    cfg: &State<LivekitConfig>,
    client: &State<RoomClient>,
) -> Result<Json<Vec<RoomInfo>>, status::Custom<String>> {
    unimplemented!();
}

#[get("/livekit")]
async fn livekit_client(
    guard: Guard,
    cfg: &State<LivekitConfig>,
) -> Result<Template, status::Custom<String>> {
    let access_token = get_access_token(&cfg, guard.claims.subject())
        .await
        .map_err(|e| {
            status::Custom(
                Status::InternalServerError,
                format!("Failed to get access token: {}", e),
            )
        })?;

    let context = json!({
        "livekit_url": cfg.base_url,
        "access_token": access_token,
    });
    Ok(Template::render("livekit", &context))
}

/// Helper to get Rocket routes from this module
pub fn get_routes() -> Vec<Route> {
    routes![token_route, list_rooms_route, livekit_client]
}
