mod init;
use init::{initial_check, master_broadcast};
mod uniclip;
use uniclip::master_uniclip;
mod unifunctions;

use tokio;
use log::{LevelFilter, error};
use env_logger;

#[tokio::main]
async fn main() {
    // env_logger::init();
    env_logger::Builder::new().filter(None, LevelFilter::Info).init();

    match initial_check().await {
        Ok(()) => {
            let broadcast_task = tokio::spawn(master_broadcast());
            let uniclip_task = tokio::spawn(master_uniclip());

            let _ = tokio::join!(broadcast_task, uniclip_task);
        }
        Err(e) => error!("Error on initial check: {}", e)
    }
}
