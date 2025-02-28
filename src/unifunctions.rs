use arboard::ImageData;
use tokio::net::UdpSocket;
use sha2::{Sha256, Digest};
use hex;
use bincode;
use serde::{Serialize, Deserialize};
use std::{error::Error, net::Ipv4Addr};
use log::{error, info};
use get_if_addrs::{get_if_addrs, IfAddr};
use crate::init::IP_REGISTER;

#[derive(Serialize, Deserialize)]
pub struct TextPacket {
    pub text: Vec<u8>,
}

pub async fn share_clip_text(item: String) -> Result<(), Box<dyn Error>> {
    let socket = UdpSocket::bind("0.0.0.0:0")
        .await.expect("Text-share UDP socket failed");

    let packet = TextPacket {
        text: item.as_bytes().to_vec(),
    };

    let message = match bincode::serialize(&packet) {
        Ok(msg) => msg,
        Err(e) => {
            error!("Serialization of txt packet failed: {}", e);
            return Err(Box::new(e));
        }
    };

    let ip_register = IP_REGISTER.lock().await;

    for ip in ip_register.iter() {
        let address = format!("{}:26025", ip);
        if let Err(e) = socket
            .send_to(&message, &address).await {
                error!("Text-share send failed to {}: {}", address, e);
                return Err(Box::new(e));
        }
    }

    Ok(())
}

#[derive(Serialize, Deserialize)]
pub struct ImagePacket {
    pub width: usize,
    pub height: usize,
    pub bytes: Vec<u8>,
    pub hash: String,
}

pub async fn share_clip_img(
    item: ImageData<'_>, 
    hash: String, 
) -> Result<(), Box<dyn Error>> {
    let socket = UdpSocket::bind("0.0.0.0:0")
        .await.expect("Img-share UDP socket failed");
    
    let packet = ImagePacket {
        width: item.width,
        height: item.height,
        bytes: item.bytes.to_vec(),
        hash
    };

    let message = match bincode::serialize(&packet) {
        Ok(msg) => msg,
        Err(e) => {
            error!("Serialization of img packet failed: {}", e);
            return Err(Box::new(e));
        }
    };

    let ip_register = IP_REGISTER.lock().await;

    for ip in ip_register.iter() {
        let address = format!("{}:26025", ip);
        if let Err(e) = socket
            .send_to(&message, &address).await {
                error!("Img-share send failed to {}: {}", address, e);
                return Err(Box::new(e));
        }
    }

    Ok(())
}

pub fn hash_img(img: &ImageData) -> String {
    let mut hasher = Sha256::new();

    hasher.update(img.width.to_le_bytes());
    hasher.update(img.height.to_le_bytes());

    for chunk in img.bytes.chunks(4096) {
        hasher.update(chunk);
    }

    hex::encode(hasher.finalize())
}

#[derive(Serialize, Deserialize)]
pub struct InitiationMessage {
    pub ip_list: Vec<String>
}

pub async fn create_initiation_message() -> Result<InitiationMessage, Box<dyn Error>> {
    let ip_register = IP_REGISTER.lock().await;

    let mut ip_list: Vec<String> = ip_register.iter().cloned().collect();

    let interfaces = get_if_addrs()?;

    for iface in interfaces {
        if iface.is_loopback() {
            continue;
        }

        if let std::net::IpAddr::V4(ip_own) = iface.ip() {
            ip_list.push(ip_own.to_string());
            info!("Own ip sent: {}", ip_own);
        }
    }

    Ok(
        InitiationMessage{
            ip_list
        }
    )
}

pub fn get_broadcast_address() -> Option<String> {
    let interfaces = get_if_addrs().ok()?;
    let mut broadcast_addr = None;

    for iface in interfaces {
        if iface.is_loopback() || iface.name.contains("vpn") || iface.name.contains("Virtual") {
            continue;
        }

        if let IfAddr::V4(ip_info) = iface.addr {
            let ip = ip_info.ip;
            let netmask = ip_info.netmask;

            if netmask.octets() == [0, 0, 0, 0] {
                continue;
            }

            let broadcast_ip = Ipv4Addr::from(u32::from(ip) | !u32::from(netmask));

            if broadcast_addr.is_none() || iface.name.contains("eth") || iface.name.contains("wlan") {
                broadcast_addr = Some(format!("{}:26025", broadcast_ip));
            }
        }
    }

    let result = broadcast_addr.unwrap_or_else(|| "255.255.255.255:26025".to_string());
    info!("Broadcast IP set to {}", result);
    Some(result)
}
