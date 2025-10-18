use ormlite::model::*;
use ormlite::sqlite::SqliteConnection;
use serde_derive::{Deserialize, Serialize};
use uuid::Uuid;
#[derive(Model, Debug, Serialize, Deserialize)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub subject: Uuid,
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
