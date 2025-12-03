use crate::config::VerdantConfig;
use crate::rooms::RoomManager;
use crate::users::User;
use crate::utils::Guard;
use crate::utils::KeyGuard;
use livekit::prelude::{Room, RoomOptions};
use livekit_api::access_token;
use livekit_api::services::room::RoomClient;
use ormlite::Connection;
use ormlite::Model;
use ormlite::postgres::PgConnection;
use reqwest::Client;
use rocket::http::Status;
use rocket::{
    Route, State, get, response::content::RawHtml, response::status, routes, serde::json::Json,
};
use rocket_dyn_templates::Template;
use rocket_oidc::CoreClaims;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use uuid::Uuid;
use verdant::livekit::TokenResponse;

use serde_json::json;

/// Lightweight configuration for communicating with LiveKit
#[derive(Model, Clone, Debug, Serialize, Deserialize)]
pub struct LiveKitServer {
    #[ormlite(primary_key)]
    id: Uuid,
    pub base_url: String, // e.g. "https://livekit.example.com"
    pub api_key: String,
    pub api_secret: String, // used to sign access tokens (HMAC)
}

/// Minimal room representation returned by LiveKit /rooms endpoint
#[derive(Deserialize, Serialize, Debug)]
pub struct LiveKitRoom {
    pub name: String,
    // add other fields you care about from the LiveKit API response
}

impl LiveKitServer {
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

        LiveKitServer {
            id: Uuid::new_v4(),
            base_url,
            api_key,
            api_secret,
        }
    }

    pub fn new(base_url: &str, api_key: &str, api_secret: &str) -> Self {
        LiveKitServer {
            id: Uuid::new_v4(),
            base_url: base_url.to_string(),
            api_key: api_key.to_string(),
            api_secret: api_secret.to_string(),
        }
    }
}

async fn get_access_token(
    cfg: &LiveKitServer,
    manager: &State<RoomManager>,
    identity: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let token = access_token::AccessToken::with_api_key(&cfg.api_key, &cfg.api_secret)
        .with_identity(identity)
        .with_name(identity)
        .with_grants(access_token::VideoGrants {
            room_join: true,
            room: "welcome".to_string(),
            ..Default::default()
        })
        .to_jwt()?;

    manager.spawn_room("welcome", &cfg.base_url, cfg).await;
    //let (room, rx) = Room::connect(&cfg.base_url, &token, RoomOptions::default()).await?;

    Ok(token)
}

/// GET /token
/// Creates a default room (if missing) and returns an access token for the participant.
/// This route is intentionally not protected (e.g., for application login flow you might protect it).
#[get("/token")]
pub async fn token_route(
    guard: KeyGuard,
    cfg: &State<LiveKitServer>,
    config: &State<VerdantConfig>,
    manager: &State<RoomManager>,
) -> Result<Json<TokenResponse>, status::Custom<String>> {
    let identity = guard.claims.subject().to_string();
    let mut conn = PgConnection::connect(&config.db_path).await.unwrap();
    let user = User::lookup_by_id(&mut conn, Uuid::from_str(&identity).unwrap())
        .await
        .unwrap();
    let token = get_access_token(&cfg, manager, &user.username)
        .await
        .map_err(|e| status::Custom(Status::InternalServerError, format!("{}", e)))?;
    println!("rpc route surl: {}", cfg.base_url);
    // for now rooms are ephemeral, but they will be kept in database in the future
    Ok(Json(TokenResponse {
        room_id: Uuid::new_v4(),
        token,
        room: identity,
        url: cfg.base_url.to_string(),
    }))
}

/// GET /rooms
/// Protected by rocket_oidc::AuthGuard — only authenticated requests allowed.
/// Returns a list of rooms from the LiveKit server.
#[get("/rooms")]
pub async fn list_rooms_route(
    guard: Guard,
    cfg: &State<LiveKitServer>,
    client: &State<RoomClient>,
) -> Result<Json<Vec<LiveKitRoom>>, status::Custom<String>> {
    unimplemented!();
}

#[get("/livekit")]
async fn livekit_client(
    guard: Guard,
    cfg: &State<LiveKitServer>,
    manager: &State<RoomManager>,
) -> Result<RawHtml<Template>, status::Custom<String>> {
    let access_token = get_access_token(&cfg, manager, guard.claims.subject())
        .await
        .map_err(|e| {
            status::Custom(
                Status::InternalServerError,
                format!("Failed to get access token: {}", e),
            )
        })?;

    // embed manifest at compile time and parse it, then attach the "index.html" entry to the template context
    let manifest_str: &str = include_str!("../static/.vite/manifest.json");
    let manifest: serde_json::Value = serde_json::from_str(manifest_str).map_err(|e| {
        status::Custom(
            Status::InternalServerError,
            format!("Failed to parse static/.vite/manifest.json: {}", e),
        )
    })?;

    let context = json!({
        "livekit_url": cfg.base_url,
        "access_token": access_token,
        "entry": manifest["index.html"]["file"].as_str().unwrap_or("static/main.js"),
        "room_name": guard.claims.subject(),
    });

    Ok(RawHtml(Template::render("livekit", &context)))
}

/// Helper to get Rocket routes from this module
pub fn get_routes() -> Vec<Route> {
    routes![token_route, list_rooms_route, livekit_client]
}
