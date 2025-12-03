use crate::config::VerdantConfig;
use crate::utils::*;
use ormlite::Model;

use rocket::{get, response::content::RawHtml, routes};
use rocket_dyn_templates::{Template, context};
use serde_derive::{Deserialize, Serialize};
use uuid::Uuid;

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

#[get("/register")]
pub fn register_page(_guard: Guard) -> RawHtml<Template> {
    RawHtml(Template::render("register", context! { title: "Register" }))
}

/// Install function to set up initial database state
/// This function is async to accommodate database operations.
pub async fn install(cfg: &VerdantConfig) {
    // This function can be used to set up database tables or initial data if needed.

    let db_path = cfg.db_path.clone();

    return;
}

pub fn get_routes() -> Vec<rocket::Route> {
    routes![login_page, register_page]
}
