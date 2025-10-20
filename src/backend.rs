use crate::rpc::LivekitConfig;
use bcrypt::verify as bcrypt_verify;
use jsonwebtoken::{Algorithm, Header, encode};
use jsonwebtoken::{DecodingKey, EncodingKey};
use ormlite::Connection;
use ormlite::model::*;
use ormlite::postgres::{PgConnection};
use rocket::http::Cookie;
use rocket_oidc::CoreClaims;
use rocket_oidc::auth::AuthGuard;

use rocket::{
    FromForm, State,
    form::Form,
    get,
    http::CookieJar,
    post,
    response::{Redirect, content::RawHtml},
    routes,
};
use rocket_dyn_templates::{Template, context};
use rocket_oidc::client::Validator;
use rocket_oidc::sign::OidcSigner;
use serde_derive::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use structopt::StructOpt;
use uuid::Uuid;

pub type Guard = rocket_oidc::auth::AuthGuard<AuthClaims>;

#[derive(Debug, Clone, StructOpt, Serialize, Deserialize)]
pub struct ConfigLoader {
    #[structopt(short, long)]
    db_path: String,
    #[structopt(short, long, parse(from_os_str))]
    key_file: PathBuf,
    #[structopt(short, long)]
    issuer_url: String,
    #[structopt(short, long, parse(from_os_str))]
    livekit_path: PathBuf,
}

impl ConfigLoader {
    pub async fn into_verdant_config(self) -> Result<VerdantConfig, Box<dyn std::error::Error>> {
        // Read and parse Livekit config
        let livekit_str = std::fs::read_to_string(&self.livekit_path)?;
        let livekit: LivekitConfig = serde_json::from_str(&livekit_str)?;

        let (privkey, pubkey) = rocket_oidc::sign::generate_rsa_pkcs8_pair();
        let encoding_key = EncodingKey::from_rsa_pem(privkey.as_bytes()).expect("invalid private key");
        let decoding_key = DecodingKey::from_rsa_pem(pubkey.as_bytes()).expect("invalid public key");   

        let signer = OidcSigner::from_rsa_pem(&privkey, "verdant")?;

        Ok(VerdantConfig {
            db_path: self.db_path,
            livekit,
            issuer_url: self.issuer_url,
            key: encoding_key,
            pubkey: decoding_key,
            signer,
        })
    }
}

pub struct VerdantConfig {
    pub db_path: String,
    pub livekit: LivekitConfig,
    pub issuer_url: String,
    pub key: EncodingKey,
    pub pubkey: DecodingKey,
    pub signer: OidcSigner,
}

impl VerdantConfig {
    /// Construct a rocket_oidc::Validator from this config's issuer URL.
    /// Returns an error if construction fails.
    pub fn validator(&self) -> Result<Validator, Box<dyn std::error::Error>> {
        // rocket_oidc::Validator::new(issuer: &str) is commonly available; adapt if your crate differs.
        let v = Validator::from_pubkey(
            self.issuer_url.clone(),
            "verdant".to_string(),
            "RS256".to_string(),
            self.pubkey.clone(),
        )?;
        Ok(v)
    }
}

#[derive(Model, Debug, Serialize, Deserialize)]
pub struct User {
    #[ormlite(primary_key)]
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub subject: Uuid,
    pub password_hash: String,
}

#[derive(Model, Debug, Serialize, Deserialize)]
pub struct Room {
    pub id: Uuid,
    pub name: String,
    pub created_at: i64,
}

/// Join table between users and rooms
/// Allows coarse LiveKit permissioning based on application-level roles
#[derive(Model, Debug)]
pub struct Permission {
    pub id: Uuid,
    pub user_id: Uuid,
    pub room_id: Uuid,
    pub room_admin: bool,
    pub can_publish: bool,
    pub can_subcribe: bool,
    //pub permissions: Vec<PermissionEntry>,
}

/// the user's permission on the room superseeds an admins ability to enable / disable a media source.
/// For example if a user / agent can't publish to a room, enabling the microphone / camera won't do anything.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionEntry {
    pub media_source: MediaSource,
    pub mode: Mode,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Mode {
    Send,
    Receive,
    Enable,
    Disable,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum MediaSource {
    Microphone,
    Camera,
    Screen,
    Speaker,
}

#[get("/login")]
async fn login_page() -> RawHtml<Template> {
    RawHtml(Template::render("login", context! { title: "Login" }))
}

#[derive(FromForm, Debug, Serialize, Deserialize)]
pub struct LoginForm {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthClaims {
    pub sub: String,
    pub exp: usize,
    pub iat: usize,
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
            exp: now + 3600,          // 1 hour expiration
            iat: now,
            iss: issuer.to_string(),
            aud: audience.to_string(),
        }
    }
}

impl CoreClaims for AuthClaims {
    fn subject(&self) -> &str {
        self.sub.as_ref()
    }
}

#[derive(FromForm, Debug, Serialize, Deserialize)]
pub struct RegisterForm {
    pub first_name: String,
    pub last_name: String,
    pub username: String,
    pub email: String,
    pub password: String,
}

#[post("/login", data = "<login_data>")]
async fn login_handler(
    config: &State<VerdantConfig>,
    cookies: &CookieJar<'_>,
    login_data: Form<LoginForm>,
) -> Redirect {
    // Open a temporary mutable connection and fetch matching user(s) via ormlite API.
    let mut conn = match PgConnection::connect(&config.db_path).await {
        Ok(c) => c,
        Err(e) => {
            println!("error: {}", e);
            return Redirect::to("/auth/login");
        }
    };

    let users = match User::select()
        .where_("username = ?")
        .bind(&login_data.username)
        .fetch_all(&mut conn)
        .await
    {
        Ok(v) => v,
        Err(e) => {
            println!("error: {}", e);
            return Redirect::to("/auth/login");
        }
    };

    let user = match users.into_iter().next() {
        Some(u) => u,
        None => return Redirect::to("/auth/login"),
    };

    // Extract stored hash and subject from the model
    let stored_hash = user.password_hash;
    let subject_str = user.subject.to_string();

    // Verify password using bcrypt
    if !bcrypt_verify(&login_data.password, &stored_hash).unwrap_or(false) {
        println!("error: invalid password");
        return Redirect::to("/auth/login");
    }

    let claims = AuthClaims::new(&subject_str, "verdant", &config.issuer_url);

    // Sign using the OidcSigner from the config (sign takes (claims, Duration))
    let token = match config
        .signer
        .sign(&claims, std::time::Duration::from_secs(3600))
    {
        Ok(t) => t,
        Err(e) => {
            println!("error: {}", e);
            return Redirect::to("/auth/login");
        }
    };

    match rocket_oidc::login(
        "/rpc/livekit".to_string(),
        cookies,
        token,
        &config.issuer_url,
        "RS256",
    ) {
        Ok(r) => r,
        Err(e) => {
            println!("error: {}", e);
            Redirect::to("/auth/login")
        }
    }
}

#[get("/register")]
pub fn register_page(guard: Guard) -> RawHtml<Template> {
    RawHtml(Template::render("register", context! { title: "Register" }))
}

#[post("/register", data = "<register_data>")]
async fn register_handler(
    guard: Guard,
    config: &State<VerdantConfig>,
    register_data: Form<RegisterForm>,
) -> Redirect {
    // Open a temporary mutable connection and insert new user via ormlite API.
    let mut conn = match PgConnection::connect(&config.db_path).await {
        Ok(c) => c,
        Err(_) => return Redirect::to("/auth/register"),
    };

    // Hash the password using bcrypt
    let password_hash = match bcrypt::hash(&register_data.password, bcrypt::DEFAULT_COST) {
        Ok(h) => h,
        Err(_) => return Redirect::to("/auth/register"),
    };

    let new_user = User {
        id: Uuid::new_v4(),
        username: register_data.username.clone(),
        email: register_data.email.clone(), // Email field can be set later
        subject: Uuid::new_v4(),
        password_hash,
    };

    if let Err(_) = new_user.insert(&mut conn).await {
        return Redirect::to("/auth/register");
    }

    Redirect::to("/auth/login")
}

/// Install function to set up initial database state
/// This function is async to accommodate database operations.
pub async fn install(cfg: &VerdantConfig) {
    // This function can be used to set up database tables or initial data if needed.

    let default_user = User {
        id: Uuid::new_v4(),
        username: "admin".to_string(),
        email: "admin@example.com".to_string(),
        subject: Uuid::new_v4(),
        password_hash: bcrypt::hash("adminpassword", bcrypt::DEFAULT_COST).unwrap(),
    };

    let db_path = cfg.db_path.clone();

    // Perform the async DB setup directly since `install` is async.
    match PgConnection::connect(&db_path).await {
        Ok(mut conn) => {
            //User::create_table(&conn).await.unwrap();

            match User::select()
                .where_("username = ?")
                .bind(&default_user.username)
                .fetch_all(&mut conn)
                .await
            {
                Ok(existing) => {
                    if existing.is_empty() {
                        let _ = default_user.insert(&mut conn).await;
                    }
                }
                Err(_) => {
                    // If the select failed for some reason, attempt an insert anyway
                    let _ = default_user.insert(&mut conn).await.unwrap();
                }
            }
        }
        Err(e) => {
            panic!("error connecting to database during install: {}", e);
        }
    };

    return;
}

pub fn get_routes() -> Vec<rocket::Route> {
    routes![login_page, login_handler, register_handler, register_page]
}
