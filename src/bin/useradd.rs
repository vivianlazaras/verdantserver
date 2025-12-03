use verdanthaven::users::AuthRecord;
// needed to estab db conn
use ormlite::Connection;
use ormlite::Model;
use ormlite::postgres::PgConnection;
use std::path::PathBuf;
use structopt::StructOpt;
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, BufReader};
use verdant::auth::register_user;
use verdanthaven::config::VerdantConfig;

pub async fn get_username_password() -> (String, String, String) {
    // Open stdin as a file for async reading
    let stdin = File::open("/dev/stdin")
        .await
        .expect("Failed to open stdin");
    let mut reader = BufReader::new(stdin);

    // Helper async closure to read a line after prompting
    async fn prompt(reader: &mut BufReader<File>, prompt: &str) -> String {
        print!("{prompt}");
        // Flush the prompt (stdout is still sync)
        use std::io::Write;
        std::io::stdout().flush().unwrap();

        let mut input = String::new();
        reader
            .read_line(&mut input)
            .await
            .expect("Failed to read line");
        input.trim().to_owned()
    }

    let username = prompt(&mut reader, "Username: ").await;
    let email = prompt(&mut reader, "Email: ").await;
    let password = prompt(&mut reader, "Password: ").await;
    let password2 = prompt(&mut reader, "Retype Password: ").await;
    if password != password2 {
        eprintln!("passwords didn't match");
        std::process::exit(-1);
    }
    (username, email, password)
}

#[derive(Debug, Clone, StructOpt)]
pub struct Args {
    #[structopt(short, long)]
    config: PathBuf,
}

#[tokio::main]
async fn main() {
    let args = Args::from_args();

    let config = VerdantConfig::load_config(&args.config).await;
    let mut conn = PgConnection::connect(&config.db_path).await.unwrap();
    let (username, email, password) = get_username_password().await;

    let registration = register_user(&config.auth_server, &username, &password).unwrap();
    let record = AuthRecord::new_user(username, email, registration);

    if cfg!(debug_assertions) {
        let record = record.insert(&mut conn).await.unwrap();
        println!("created record: {:?}", record);
    } else {
        record.insert(&mut conn).await.unwrap();
    }
}
