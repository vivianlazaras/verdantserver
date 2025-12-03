use crate::errors::*;
use crate::rpc::LiveKitServer;
use jsonwebtoken::{DecodingKey, EncodingKey};
use keycast::crypto::sha2_impl::Sha256Alg;
use keycast::crypto::sha2_impl::Sha512Alg;
use keycast::crypto::*;
use keycast::discovery::*;
use qrcode::QrCode;
use qrcode::render::unicode;
use rocket_oidc::client::Validator;
use rocket_oidc::sign::OidcSigner;
use rsa::pkcs1::EncodeRsaPublicKey;
use rsa::pkcs8::DecodePrivateKey;
use rsa::pkcs8::DecodePublicKey;
use serde_derive::{Deserialize, Serialize};
use std::io;
use std::net::IpAddr;
use std::path::Path;
use std::path::PathBuf;
use structopt::StructOpt;
use time::{Duration, OffsetDateTime};
use tokio::fs::File;
use tokio::io::AsyncReadExt; // for completeness
use verdant::server::auth::ServerSetup;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PgConfig {
    user: String,
    host: String,
    password: String,
    dbname: String,
    port: u16,
}

impl DBConfig for PgConfig {
    fn into_url(self) -> String {
        format!(
            "postgresql://{}:{}@{}:{}/{}",
            self.user, self.password, self.host, self.port, self.dbname
        )
    }
}

pub trait DBConfig {
    fn into_url(self) -> String;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemConfigLoader {
    db_url: String,
    cert_key: String,
    setup: ServerSetup,
    issuer_url: Option<String>,
    livekit: Vec<LiveKitServer>,
    port: u16,
    addr: Option<IpAddr>,
}

async fn read_setup_file(path: impl AsRef<Path>) -> Result<ServerSetup, crate::errors::VerdantErr> {
    // Open the file asynchronously
    let mut file = File::open(path).await?;

    // Read the file into a buffer
    let mut contents = Vec::new();
    file.read_to_end(&mut contents).await?;

    // Deserialize contents into ServerSetup
    let setup = ServerSetup::deserialize(&contents)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

    Ok(setup)
}

#[derive(Debug, Clone, StructOpt, Serialize, Deserialize)]
pub struct ConfigLoader {
    #[structopt(short, long)]
    db_path: String,
    #[structopt(short, long, parse(from_os_str))]
    key_file: PathBuf,
    #[structopt(short, long)]
    setup_file: PathBuf,
    #[structopt(short, long)]
    issuer_url: String,
    #[structopt(short, long, parse(from_os_str))]
    livekit_path: PathBuf,
    #[structopt(short, long)]
    port: u16,
    #[structopt(short, long)]
    addr: Option<IpAddr>,
    /// whether or not to advertise this verdant instance using mdns_sd
    #[structopt(short, long)]
    advertise: Option<bool>,
}

impl ConfigLoader {
    pub async fn new_enc_config(db_url: impl Into<String>) -> MemConfigLoader {
        unimplemented!();
    }

    pub async fn into_verdant_config(self) -> Result<VerdantConfig, VerdantErr> {
        // Read and parse Livekit config
        let livekit_str = std::fs::read_to_string(&self.livekit_path)?;
        let livekit: LiveKitServer = serde_json::from_str(&livekit_str)?;

        let (privkey, pubkey) = rocket_oidc::sign::generate_rsa_pkcs8_pair();
        let rsa_pubkey = rsa::RsaPublicKey::from_public_key_pem(&pubkey)?;
        let rsa_privkey = rsa::RsaPrivateKey::from_pkcs8_pem(&privkey)?;
        let encoding_key =
            EncodingKey::from_rsa_pem(privkey.as_bytes()).expect("invalid private key");
        let decoding_key =
            DecodingKey::from_rsa_pem(pubkey.as_bytes()).expect("invalid public key");

        let signer = OidcSigner::from_rsa_pem(&privkey, "verdant")?;

        let setup = read_setup_file(&self.setup_file).await?;

        Ok(VerdantConfig {
            db_path: self.db_path,
            livekit,
            issuer_url: self.issuer_url,
            key: encoding_key,
            pubkey: decoding_key,
            rsa_pubkey,
            rsa_privkey,
            signer,
            auth_server: verdant::server::auth::Server::new(setup),
            port: self.port,
            addr: self.addr.unwrap_or_else(|| "0.0.0.0".parse().unwrap()),
            advertise: self.advertise.unwrap_or_else(|| false),
        })
    }
}

pub struct VerdantConfig {
    pub db_path: String,
    pub livekit: LiveKitServer,
    pub issuer_url: String,
    pub key: EncodingKey,
    pub pubkey: DecodingKey,
    pub rsa_pubkey: rsa::RsaPublicKey,
    pub rsa_privkey: rsa::RsaPrivateKey,
    pub signer: OidcSigner,
    pub auth_server: verdant::server::auth::Server,
    pub port: u16,
    pub addr: IpAddr,
    pub advertise: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FingerPrint {
    pub keyhash: String,
    pub url: String,
}

impl VerdantConfig {
    /// Construct a rocket_oidc::Validator from this config's issuer URL.
    /// Returns an error if construction fails.
    pub fn validator(&self) -> Result<Validator, VerdantErr> {
        // rocket_oidc::Validator::new(issuer: &str) is commonly available; adapt if your crate differs.
        let v = Validator::from_pubkey(
            self.issuer_url.clone(),
            "verdant".to_string(),
            "RS256".to_string(),
            self.pubkey.clone(),
        )
        .unwrap();
        Ok(v)
    }

    pub fn rocket_config(&self) -> rocket::Config {
        let mut config = rocket::Config::default();
        config.address = self.addr;
        config.port = self.port;
        config
    }

    pub async fn to_beacon(&self) -> Result<Beacon, keycast::errors::BeaconError> {
        let ident = ServiceIdent::TCP("verdant".to_string());
        let hash = KeyHash::from_pubkey(self.rsa_pubkey.clone(), &Sha512Alg, Encoding::Base64Der)?;
        let mut beacon = Beacon::new(ident, hash).await;
        beacon.port = self.port;
        beacon.protocol = WebProtocol::Http;
        Ok(beacon)
    }

    pub async fn load_config(path: &Path) -> Self {
        // Load and parse the JSON config file into your ConfigLoader type
        let file = std::fs::File::open(path).expect("failed to open config file");
        let loader: ConfigLoader =
            serde_json::from_reader(file).expect("failed to parse config JSON");

        // Extract the LiveKitServer and Validator from the loader.
        // Adjust method names if your ConfigLoader API differs.
        println!("loader: {:?}", loader);
        let verdant_config = loader
            .into_verdant_config()
            .await
            .expect("failed to build verdant config");

        verdant_config
    }

    /// later this will be made into a config option
    fn default_cert_lifetime() -> (OffsetDateTime, OffsetDateTime) {
        let day = Duration::new(86400, 0);
        let yesterday = OffsetDateTime::now_utc().checked_sub(day).unwrap();
        let days45 = OffsetDateTime::now_utc().checked_add(day * 45).unwrap();
        (yesterday, days45)
    }

    pub fn generate_certificate_der(
        &self,
        extra_names: Vec<String>,
    ) -> Result<Vec<u8>, crate::errors::VerdantErr> {
        let lifetime = Self::default_cert_lifetime();
        let hasher = keycast::crypto::sha2_impl::Sha256Alg;
        let (_, _, cert) = keycast::crypto::certgen::generate_self_signed_cert(
            &hasher,
            &self.rsa_privkey,
            lifetime,
            extra_names,
        )?;
        Ok(cert.der().to_vec())
    }

    pub fn der_certificate_unicode_qr(
        &self,
        extra_names: Vec<String>,
    ) -> Result<String, crate::errors::VerdantErr> {
        let cert_der = self.generate_certificate_der(extra_names)?;
        let code = QrCode::new(&cert_der)?;
        let image = code
            .render::<unicode::Dense1x2>()
            .dark_color(unicode::Dense1x2::Light)
            .light_color(unicode::Dense1x2::Dark)
            .build();
        Ok(image)
    }

    pub fn der_pubkey_unicode_qr(&self) -> Result<String, crate::errors::VerdantErr> {
        let der = self.rsa_pubkey.to_pkcs1_der().unwrap();
        let code = QrCode::new(&der)?;
        let image = code
            .render::<unicode::Dense1x2>()
            .dark_color(unicode::Dense1x2::Light)
            .light_color(unicode::Dense1x2::Dark)
            .build();
        Ok(image)
    }

    pub fn der_pubkey_hash_unicode_qr(&self) -> Result<String, crate::errors::VerdantErr> {
        let keyhash =
            KeyHash::from_pubkey(self.rsa_pubkey.clone(), &Sha256Alg, Encoding::Base64Der)
                .unwrap()
                .to_string();
        println!("keyhash len: {}", keyhash.len());
        let image = QrCode::new(keyhash)?
            .render::<unicode::Dense1x2>()
            .dark_color(unicode::Dense1x2::Light)
            .light_color(unicode::Dense1x2::Dark)
            .build();
        Ok(image)
    }
}
