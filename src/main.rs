use rocket::fs::{FileServer, relative};
use rocket::{State, catch, catchers, launch, response::Redirect, routes};
use rocket_dyn_templates::Template;

use keycast::discovery::Beacon;
use mdns_sd::DaemonEvent;
use rocket::get;
use rocket::serde::json::Json;
use spki::EncodePublicKey;
use std::path::PathBuf;
use structopt::StructOpt;
use verdant::api::{KeyType, PubKeyResponse};
use verdanthaven::backend::install;
use verdanthaven::config::VerdantConfig;
use verdanthaven::rooms::*;

#[get("/pubkey")]
async fn get_pubkey(config: &State<VerdantConfig>) -> Json<PubKeyResponse> {
    let der = config.rsa_pubkey.to_public_key_der().unwrap().into_vec();
    let response = PubKeyResponse::encode_pubkey(KeyType::Rsa, &der);
    Json(response)
}

#[derive(Debug, Clone, StructOpt)]
pub struct Args {
    #[structopt(short, long)]
    pub config: PathBuf,
}

#[catch(401)]
fn unauthorized() -> Redirect {
    Redirect::to("/auth/login")
}

async fn start_advertising(beacon: Beacon) {
    println!("beacon: {:?}", beacon);
    let handle = beacon.advertise().await.unwrap();
    tokio::spawn(async move {
        println!("[Advertiser] Beacon broadcasting. Press Ctrl+C to exit.");

        println!("[Advertiser] Shutting down.");

        //handle.multicast.abort();
        while let Ok(event) = handle.monitor.recv() {
            println!("Daemon event: {:?}", &event);
            if let DaemonEvent::Error(e) = event {
                println!("Failed: {}", e);
                break;
            }
        }
    });
}

#[launch]
async fn rocket() -> _ {
    let args = Args::from_args();
    let cfg = VerdantConfig::load_config(&args.config).await;
    let validator = cfg.validator().expect("failed to create OIDC validator");

    // Your LiveKit configuration
    let livecfg = cfg.livekit.clone();

    let room_client = livecfg.room_client();

    // Rocket configuration
    let rocket_cfg = cfg.rocket_config();

    let beacon = cfg.to_beacon().await.unwrap();
    let external_ip = beacon.ip.unwrap();
    let external_url = format!("https://{}:{}", external_ip, cfg.port);

    if cfg.advertise {
        start_advertising(beacon).await
    }

    let qrcode = cfg.der_certificate_unicode_qr(vec![external_url]).unwrap();
    println!("DER encoded certificate \n{}\n", qrcode);

    let pubkeyqr = cfg.der_pubkey_unicode_qr().unwrap();
    println!("DER encoded pubkey: \n{}\n", pubkeyqr);

    let keyhashqr = cfg.der_pubkey_hash_unicode_qr().unwrap();
    println!(
        "DER + Base64 Sha2-256 hash of public key: \n{}\n",
        keyhashqr
    );
    install(&cfg).await;

    let session = ServerSession::from_root("./sessions");
    let roommgmr = RoomManager::new(session);

    rocket::custom(rocket_cfg)
        .attach(Template::fairing())
        .register("/", catchers![unauthorized])
        .manage(validator)
        .manage(cfg)
        .manage(livecfg)
        .manage(roommgmr)
        .manage(room_client)
        .mount("/", routes![get_pubkey])
        .mount("/rpc", verdanthaven::rpc::get_routes())
        .mount("/users", verdanthaven::backend::get_routes())
        .mount("/auth", verdanthaven::users::get_routes())
        .mount("/", FileServer::from(relative!("static")))
}
