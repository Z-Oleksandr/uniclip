mod init;
use init::{initial_check, master_broadcast};
mod uniclip;
use uniclip::master_uniclip;
mod unifunctions;
mod firewall;
use firewall::add_firewall_rule;

use tokio;
use log::{LevelFilter, error, warn};
use env_logger;

#[tokio::main]
async fn main() {
    // env_logger::init();
    env_logger::Builder::new().filter(None, LevelFilter::Info).init();

    if let Err(e) = add_firewall_rule(26025) {
        warn!("Failed to add Firewall rule. Please open port 26025 manually. {}", e);
    }

    match initial_check().await {
        Ok(()) => {
            let broadcast_task = tokio::spawn(master_broadcast());
            let uniclip_task = tokio::spawn(master_uniclip());

            let _ = tokio::join!(broadcast_task, uniclip_task);
        }
        Err(e) => error!("Error on initial check: {}", e)
    }
}
