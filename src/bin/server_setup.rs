use rand::rngs::OsRng;
use std::path::PathBuf;
use structopt::StructOpt;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use verdant::server::auth::ServerSetup;

#[derive(Debug, StructOpt)]
pub struct Args {
    #[structopt(short, long)]
    path: PathBuf,
}

#[tokio::main]
async fn main() {
    let args = Args::from_args();

    let mut rng = OsRng;
    let setup = ServerSetup::new(&mut rng);
    let serialized = setup.serialize().to_vec();
    let mut file = File::create(args.path).await.unwrap();
    file.write_all(&serialized).await.unwrap();
}
