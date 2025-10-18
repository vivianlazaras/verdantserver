use rocket::fs::{FileServer, relative};
use rocket::{State, launch, routes};
use rocket_dyn_templates::Template;

#[launch]
fn rocket() -> _ {
    use rocket::config::Config;

    // Your LiveKit configuration
    let cfg = verdanthaven::rpc::LivekitConfig::new(
        "http://localhost:7880",
        "verdanthaven",
        "oKutoHbKUKUdSp27JwV4pKfhgzTLLsnE9GBI8RTxcuC",
    );

    let room_client = cfg.room_client();

    // Rocket configuration
    let rocket_cfg = Config {
        address: "0.0.0.0".parse().unwrap(),
        port: 8080, // ← change this to whatever port you want
        ..Config::debug_default()
    };

    rocket::custom(rocket_cfg)
        .attach(Template::fairing())
        .manage(cfg)
        .manage(room_client)
        .mount("/rpc", verdanthaven::rpc::get_routes())
        .mount("/", FileServer::from(relative!("static")))
}
