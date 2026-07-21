mod cmd;
mod config;
mod keychain;
mod server;
mod state;
mod ui;

#[tokio::main(flavor = "current_thread")]
pub async fn main() {
    if let Err(err) = cmd::start().await {
        eprintln!("{err:?}");
        std::process::exit(1);
    }
}
