use thiserror::Error;
#[derive(Debug, Error)]
pub enum VerdantErr {
    #[error("username not found")]
    MissingUsername,
    #[error("too many auth records found")]
    TooManyRecords,
    #[error("too many user records found for username: {0}")]
    TooManyUser(String),
    #[error("auth record not found")]
    RecordNotFound,
    #[error("OrmLite Error: {0}")]
    SqlxError(#[from] ormlite::SqlxError),
    #[error("jsonwebtoken error: {0}")]
    WebTokenErr(#[from] jsonwebtoken::errors::Error),
    #[error("serde_json error: {0}")]
    JsonErr(#[from] serde_json::Error),
    #[error("IO Error: {0}")]
    IOError(#[from] std::io::Error),
    #[error("PAKE Error: {0}")]
    OpaqueKe(#[from] verdant::errors::ProtocolError),
    #[error("base64 decode error: {0}")]
    Base64Decode(#[from] base64::DecodeError),
    #[error("ormlite error: {0}")]
    OrmLiteErr(#[from] ormlite::Error),
    #[error("protocol error: {0}")]
    VerdantErr(#[from] verdant::errors::Error),
    #[error("PKCS8 decoding error: {0}")]
    RsaPkcs8Err(#[from] rsa::pkcs8::spki::Error),
    #[error("PKCS8 privkey decoding error: {0}")]
    RsaPrivPkcs8Err(#[from] rsa::pkcs8::Error),
    #[error("certificate generation error: {0}")]
    CertGenErr(#[from] keycast::crypto::certgen::CertGenError),
    #[error("failed to generate QRcode")]
    QRCodeGenError(#[from] qrcode::types::QrError),
    #[error("failed to connect to room: {0}")]
    RoomErr(#[from] livekit::RoomError),
    #[error("failed to start async task")]
    JoinError(#[from] tokio::task::JoinError),
    #[error("failed to create writer: {0}")]
    WavError(#[from] hound::Error),
}
