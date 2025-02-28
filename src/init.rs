use tokio::{net::UdpSocket, sync::Mutex, time::{self, timeout, Duration}};
use lazy_static::lazy_static;
use log::{error, info};
use bincode;
use std::error::Error;

use crate::unifunctions::{
    ImagePacket, TextPacket, 
    create_initiation_message,
    InitiationMessage,
    get_broadcast_address
};
use crate::uniclip::{handle_incoming_txt, handle_incoming_img};

lazy_static! {
    pub static ref IP_REGISTER: Mutex<Vec<String>> = Mutex::new(Vec::new());
}

pub async fn initial_check() -> Result<(), Box<dyn Error>> {
    let socket = UdpSocket::bind("0.0.0.0:26025")
        .await.expect("Bind client socket failed");
    socket.set_broadcast(true).expect("Enable broadcast failed");

    let broadcast_addr = get_broadcast_address().unwrap();
    let message = b"DISCOVER_SIGNAL";

    if let Err(e) = socket.send_to(message, broadcast_addr)
        .await {
            error!("Send broadcast failed: {}", e);
            return Err(Box::new(e));
        };
    info!("Broadcasting discovery message...");

    let mut buf = [0; 2048];
    let mut found_network = false;
    let start_time = time::Instant::now();
    while start_time.elapsed() < Duration::from_secs(2) {
        match timeout(Duration::from_millis(500), socket.recv_from(&mut buf)).await {
            Ok(Ok((size, _src))) => {
                let response = &buf[..size];
                if let Ok(init_msg) = bincode::deserialize::<InitiationMessage>(response) {
                    let mut ip_register = IP_REGISTER.lock().await;
                    for ip in init_msg.ip_list {
                        if !ip_register.contains(&ip) {
                            ip_register.push(ip);
                        }
                    }
                    found_network = true;
                }
            }
            Ok(Err(e)) => {
                error!("Error receiving init_msg: {}", e);
            }
            Err(_) => {
                break;
            }
        }
    }

    if found_network {
        info!("Connected to network!");
    } else {
        info!("No response, starting solo...");
    }
    Ok(())
}

pub async fn master_broadcast() {
    let socket = match UdpSocket::bind("0.0.0.0:26025")
        .await {
            Ok(s) => s,
            Err(e) => {
                error!("UDP socket bind failed: {}", e);
                return;
            }
        };

    info!("Server broadcast listening on port 26025...");

    let mut buf = [0; 65535];
    loop {
        let (size, src) = match
            socket.recv_from(&mut buf).await {
                Ok(res) => res,
                Err(e) => {
                    error!("UDP receive error: {}", e);
                    continue;
                }
            };

        let received = &buf[..size];

        if received == b"DISCOVER_SIGNAL" {
            info!("DISCOVER_SIGNAL received from {}", src);

            let init_msg = match create_initiation_message().await {
                Ok(msg) => msg,
                Err(e) => {
                    error!("Error creating init message: {}", e);
                    return;
                }
            };

            let init_msg_pkg = match bincode::serialize(&init_msg) {
                Ok(pkg) => pkg,
                Err(e) => {
                    error!("Error serializing init message: {}", e);
                    return;
                }
            };

            if let Err(e) = socket.send_to(&init_msg_pkg, src)
                .await {
                    error!("InitMsg send failed: {}", e);
                }
            add_ip(src.ip().to_string()).await;
        } else if let Ok(text_packet) = bincode::deserialize::<TextPacket>(received) {
            if let Err(e) = handle_incoming_txt(
                String::from_utf8_lossy(&text_packet.text).into_owned()
            ).await {
                error!("Failed to handle incoming text from {}: {}", src, e);
            }
        } else if let Ok(image_packet) = bincode::deserialize::<ImagePacket>(received) {
            if let Err(e) = handle_incoming_img(
                image_packet
            ).await {
                error!("Failed to handle incoming img from {}: {}", src, e);
            }
        } else {
            error!("Received unknown packet from {}", src);
        }
    }
}

async fn add_ip(ip: String) {
    let mut ip_register = IP_REGISTER.lock().await;

    if !ip_register.contains(&ip.to_string()) {
        ip_register.push(ip.to_string());
        info!("Added new slave: {}", ip);
    } else {
        info!("Slave already exists: {}", ip);
    }
}
