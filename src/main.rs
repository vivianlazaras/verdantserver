use rocket::fs::{FileServer, relative};
use rocket::{Request, State, catch, catchers, launch, response::Redirect, routes};
use rocket_dyn_templates::Template;

use serde_json;
use std::fs::File;
use std::path::Path;
use std::path::PathBuf;
use structopt::StructOpt;
use verdanthaven::backend::install;
use verdanthaven::backend::{ConfigLoader, VerdantConfig};
use verdanthaven::rpc::LivekitConfig;

#[derive(Debug, Clone, StructOpt)]
pub struct Args {
    #[structopt(short, long)]
    pub config_file: PathBuf,
}

async fn load_config(
    path: &Path,
) -> (
    verdanthaven::backend::VerdantConfig,
    rocket_oidc::client::Validator,
) {
    // Load and parse the JSON config file into your ConfigLoader type
    let file = File::open(path).expect("failed to open config file");
    let loader: ConfigLoader = serde_json::from_reader(file).expect("failed to parse config JSON");

    // Extract the LivekitConfig and Validator from the loader.
    // Adjust method names if your ConfigLoader API differs.
    println!("loader: {:?}", loader);
    let verdant_config = loader
        .into_verdant_config()
        .await
        .expect("failed to build verdant config");
    let validator = verdant_config
        .validator()
        .expect("failed to create OIDC validator");

    (verdant_config, validator)
}

#[catch(401)]
fn unauthorized() -> Redirect {
    Redirect::to("/auth/login")
}

#[launch]
async fn rocket() -> _ {
    use rocket::config::Config;

    let args = Args::from_args();
    let (cfg, validator) = load_config(&args.config_file).await;

    // Your LiveKit configuration
    let livecfg = verdanthaven::rpc::LivekitConfig::new(
        "http://localhost:7880",
        "verdanthaven",
        "oKutoHbKUKUdSp27JwV4pKfhgzTLLsnE9GBI8RTxcuC",
    );

    let room_client = livecfg.room_client();

    // Rocket configuration
    let rocket_cfg = Config {
        address: "0.0.0.0".parse().unwrap(),
        port: 8080, // ← change this to whatever port you want
        ..Config::debug_default()
    };

    install(&cfg).await;

    rocket::custom(rocket_cfg)
        .attach(Template::fairing())
        .register("/", catchers![unauthorized])
        .manage(validator)
        .manage(cfg)
        .manage(livecfg)
        .manage(room_client)
        .mount("/rpc", verdanthaven::rpc::get_routes())
        .mount("/auth", verdanthaven::backend::get_routes())
        .mount("/", FileServer::from(relative!("static")))
}
