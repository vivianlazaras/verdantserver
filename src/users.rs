use crate::config::VerdantConfig;
use crate::errors::*;
use crate::utils::*;
use bcrypt::verify as bcrypt_verify;
use ormlite::Connection;
use ormlite::Model;
use ormlite::model::*;
use ormlite::postgres::PgConnection;
use rocket::response::status;
use rocket::serde::json::Json;
use rocket::{FromForm, State, form::Form, http::CookieJar, post, response::Redirect, routes};
use serde_derive::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;
use verdant::auth::LoginResult;
use verdant::auth::challenge::LoginCompletion;
use verdant::auth::challenge::LoginUpload;
use verdant::auth::challenge::Transcript;
use verdant::client::auth::LoginRequest;
use verdant::server::auth::*;
use verdant::utils::*;

pub type Guard = rocket_oidc::auth::AuthGuard<AuthClaims>;
pub type KeyGuard = rocket_oidc::auth::ApiKeyGuard<AuthClaims>;

pub fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs() as i64
}
// this struct records metadata only, it doesn't handle actually verifying a session's validity.
#[derive(Debug, Serialize, Deserialize, Model)]
struct LoginSession {
    #[ormlite(primary_key)]
    id: Uuid,
    #[ormlite(column = "user_id")]
    user: Join<User>,
    // stores the opaque-ke serialized [`ServerLogin`] object.
    server_login: String,
    // stores a transcript of the current exchange so far as (`LoginRequest`, `LoginResponse`) in bincode encoded base64 string.
    transcript: String,
    // When the login is completed and shared session key is derived.
    login_start: Option<i64>,
    // when this record is created.
    session_start: i64,
    // when this session is due to expire.
    session_end: i64,
}

impl LoginSession {
    pub fn new(
        user: User,
        server_login: ServerLogin,
        request: &LoginRequest,
        response: &CredentialResponse,
    ) -> Self {
        let id = Uuid::new_v4();
        let response = LoginResponse::PAKE(Box::new((id, response.clone())));
        let transcript =
            verdant::auth::challenge::Transcript::compute_transcript(&request, &response);
        let transcript_str = transcript.to_string();
        let login = base64_encode(&server_login.serialize());
        Self {
            id,
            transcript: transcript_str,
            user: Join::new(user),
            server_login: login,
            login_start: None,
            session_start: unix_now(),
            session_end: unix_now() + 3600,
        }
    }
    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn decode_transcript(&self) -> Result<Transcript, crate::errors::VerdantErr> {
        Ok(Transcript::decode(&self.transcript)?)
    }

    pub fn decode_login(&self) -> Result<ServerLogin, crate::errors::VerdantErr> {
        Ok(ServerLogin::deserialize(&base64_decode(
            &self.server_login.as_bytes(),
        )?)?)
    }

    pub fn finalize(
        &self,
        upload: LoginUpload,
        config: &VerdantConfig,
    ) -> Result<LoginCompletion, crate::errors::VerdantErr> {
        // first decode transcript.
        let transcript = self.decode_transcript()?;
        // then decode server login.
        let login = self.decode_login()?;
        // verifies the message the client sent before returning CredentialFinalization
        let finalization = upload.finalization();
        let key = config.auth_server.finish_login(login, finalization)?;
        if !upload.verify_transcript(&key, &transcript) {
            return Ok(LoginCompletion::unauthorized());
        }
        let token = AuthClaims::issue_jwt(config, &self.user.id.to_string()).unwrap();
        let result = LoginResult::Success(token);
        let success = LoginCompletion::new(result, &key, transcript);
        Ok(success)
    }
}

#[derive(Model, Debug, Serialize, Deserialize)]
pub struct User {
    #[ormlite(primary_key)]
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub subject: Uuid,
    /// is this account associated with a human, or a bot (aka can they be made to unmute / show video remotely)
    pub human: bool,
}

impl User {
    async fn lookup_user(conn: &mut PgConnection, username: &str) -> Result<User, VerdantErr> {
        let mut users = User::select()
            .where_("username = ?")
            .bind(username)
            .fetch_all(conn)
            .await?;

        if users.len() > 1 {
            Err(VerdantErr::TooManyUser(username.to_string()))
        } else if users.len() < 1 {
            Err(VerdantErr::MissingUsername)
        } else {
            Ok(users
                .pop()
                .expect("impossible situation poping Vec of length 1"))
        }
    }
    pub(crate) async fn lookup_by_id(
        conn: &mut PgConnection,
        id: Uuid,
    ) -> Result<User, VerdantErr> {
        let mut users = User::select()
            .where_("id = ?")
            .bind(id)
            .fetch_all(conn)
            .await?;

        if users.len() > 1 {
            Err(VerdantErr::TooManyUser(id.to_string()))
        } else if users.len() < 1 {
            Err(VerdantErr::MissingUsername)
        } else {
            Ok(users
                .pop()
                .expect("impossible situation poping Vec of length 1"))
        }
    }
}

#[derive(Model, Debug, Serialize, Deserialize)]
pub struct AuthRecord {
    #[ormlite(primary_key)]
    pub id: Uuid,
    #[ormlite(column = "user_id")]
    pub user: Join<User>,
    /// password hashes should only exist as temporary passwords
    pub password_hash: Option<String>,
    /// when this auth record is to be automatically deleted.
    pub expiration: i64,
    /// base64 encoded [`opaque_ke::ServerRegistration`]
    registration: Option<String>,
}

impl AuthRecord {
    pub fn new_user(
        username: impl Into<String>,
        email: impl Into<String>,
        registration: ServerRegistration,
    ) -> Self {
        let user_id = Uuid::new_v4();
        let user = User {
            id: user_id,
            subject: user_id,
            username: username.into(),
            email: email.into(),
            human: true,
        };

        let bytes = registration.serialize().as_slice().to_vec();
        let credentials = base64_encode(&bytes);

        let record = AuthRecord {
            id: Uuid::new_v4(),
            user: Join::new(user),
            expiration: 0,
            registration: Some(credentials),
            password_hash: None,
        };

        record
    }

    pub fn registration(&self) -> Result<Option<ServerRegistration>, VerdantErr> {
        let bytes = base64_decode(match &self.registration {
            Some(r) => r.as_bytes(),
            None => return Ok(None),
        })?;
        Ok(Some(match ServerRegistration::deserialize(&bytes) {
            Ok(val) => val,
            Err(err) => return Err(VerdantErr::OpaqueKe(err)),
        }))
    }
    // returns an [`AuthRecord`] matching [`username`]
    async fn get_auth_record(
        conn: &mut PgConnection,
        username: &str,
    ) -> Result<AuthRecord, VerdantErr> {
        Ok(
            match AuthRecord::select()
                .join(AuthRecord::user())
                .where_("username = ?")
                .bind(username)
                .fetch_all(&mut *conn)
                .await
            {
                Ok(mut v) => {
                    if v.len() < 1 {
                        return Err(VerdantErr::MissingUsername);
                    } else if v.len() > 1 {
                        return Err(VerdantErr::TooManyRecords);
                    }
                    let mut val = v.pop().unwrap();
                    // delete password hash, if the server fails or user doesn't reset password before
                    // token expires the user will be locked out.
                    if val.password_hash.is_some() {
                        val.password_hash = None;
                        val = val.update_all_fields(&mut *conn).await?;
                    }
                    val
                }
                Err(e) => {
                    println!("error: {}", e);
                    return Err(VerdantErr::RecordNotFound);
                }
            },
        )
    }
}

async fn start_opaque_login(
    config: &VerdantConfig,
    conn: &mut PgConnection,
    login_data: &LoginRequest,
    registration: ServerRegistration,
) -> Result<(Uuid, CredentialResponse), VerdantErr> {
    let request =
        CredentialRequest::deserialize(&base64_decode(login_data.credentials.as_bytes())?)?;
    let (login, response) =
        config
            .auth_server
            .start_login(registration, request, &login_data.username)?;
    // grab user next to store in session
    let user = User::lookup_user(conn, &login_data.username).await?;
    let session = LoginSession::new(user, login, &login_data, &response);
    let id = session.id();
    session.insert(conn).await?;
    Ok((id, response))
}

#[derive(FromForm, Debug, Serialize, Deserialize)]
pub struct LoginForm {
    pub username: String,
    /// can either be a credential request or a password.
    pub credentials: String,
}

impl LoginForm {
    pub fn into_login_request(self) -> LoginRequest {
        LoginRequest {
            username: self.username,
            credentials: self.credentials,
        }
    }
}

async fn otp_login(
    config: &VerdantConfig,
    record: &AuthRecord,
    login_data: &LoginRequest,
) -> Option<String> {
    // Verify password using bcrypt
    if !bcrypt_verify(
        &login_data.credentials,
        &record.password_hash.as_ref().unwrap(),
    )
    .unwrap_or(false)
    {
        println!("error: invalid password");
        return None;
    }
    let id = record.user.id.to_string();
    AuthClaims::issue_jwt(config, &id)
}

// this impl uses password hashing removing in favor of PAKE auth.
// eventually this should return a OTP response (One Time Password).
async fn login(
    config: &VerdantConfig,
    login_data: LoginRequest,
) -> Result<LoginResponse, VerdantErr> {
    let mut conn = PgConnection::connect(&config.db_path).await?;

    let record = AuthRecord::get_auth_record(&mut conn, &login_data.username).await?;

    // Extract stored hash and subject from the model
    let stored_hash = &record.password_hash;
    let subject_str = &record.user.id.to_string();

    let login_response = if let Some(hash) = stored_hash {
        if let Some(reg) = record.registration()? {
            // both password hash and registration are set, this shouldn't happen, but can be handled
            // by deleting the password_hash, and continuing with the registration.

            match start_opaque_login(config, &mut conn, &login_data, reg).await {
                Ok((id, token)) => LoginResponse::PAKE(Box::new((id, token))),
                Err(e) => {
                    eprintln!("opaque login error: {}", e);
                    LoginResponse::AccessDenied
                }
            }
        } else {
            // only password hash is set
            // continue with OTP login, then return a LoginResponse::OTP
            // which indicates to the the client that password reset / registration should be performed,
            // while they have a valid access token.
            match otp_login(config, &record, &login_data).await {
                Some(token) => LoginResponse::OTP(token),
                None => LoginResponse::AccessDenied,
            }
        }
    } else if let Some(reg) = record.registration()? {
        match start_opaque_login(config, &mut conn, &login_data, reg).await {
            Ok((id, response)) => LoginResponse::PAKE(Box::new((id, response))),
            Err(e) => {
                eprintln!("opaque login error: {}", e);
                LoginResponse::AccessDenied
            }
        }
        // only registration is set, proceed with normal authentication.
    } else {
        eprintln!("no viable authenticaiton method");
        LoginResponse::AccessDenied
    };

    Ok(login_response)
}

#[post("/api/login", data = "<login_data>")]
async fn api_login_handler(
    config: &State<VerdantConfig>,
    login_data: Json<LoginForm>,
) -> Result<Json<LoginResponse>, status::Unauthorized<String>> {
    let data = login_data.into_inner().into_login_request();
    Ok(Json(match login(&config, data).await {
        Ok(data) => data,
        Err(e) => {
            eprintln!("login error: {}", e);
            return Ok(Json(LoginResponse::AccessDenied));
        }
    }))
}

#[post("/api/login/finalize", data = "<login_upload>")]
async fn complete_api_login(
    config: &State<VerdantConfig>,
    login_upload: Json<LoginUpload>,
) -> Json<LoginCompletion> {
    let mut conn = PgConnection::connect(&config.db_path).await.unwrap();
    let upload = login_upload.into_inner();
    // 1. Fetch the session from DB
    let session: Option<LoginSession> = LoginSession::select()
        .where_("id = ?")
        .bind(upload.id)
        .fetch_optional(&mut conn)
        .await
        .expect("query failed");

    let session = match session {
        Some(s) => s,
        None => return Json(LoginCompletion::unauthorized()),
    };

    Json(session.finalize(upload, config).unwrap())
}

#[post("/login", data = "<login_data>")]
async fn login_handler(
    config: &State<VerdantConfig>,
    cookies: &CookieJar<'_>,
    login_data: Form<LoginForm>,
) -> Redirect {
    unimplemented!();
}

#[derive(FromForm, Debug, Serialize, Deserialize)]
pub struct RegisterForm {
    pub first_name: String,
    pub last_name: String,
    pub username: String,
    pub email: String,
}

#[post("/register", data = "<register_form>")]
async fn register_handler(
    guard: Guard,
    config: &State<VerdantConfig>,
    register_form: Form<RegisterForm>,
) -> Redirect {
    Redirect::to("/auth/login")
}

pub fn get_routes() -> Vec<rocket::Route> {
    routes![
        login_handler,
        complete_api_login,
        api_login_handler,
        register_handler
    ]
}
