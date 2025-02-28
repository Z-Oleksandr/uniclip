use arboard::ImageData;
use tokio::{net::UdpSocket, time::{sleep, Duration, Instant}};
use sha2::{Sha256, Digest};
use hex;
use bincode;
use serde::{Serialize, Deserialize};
use std::{error::Error, net::Ipv4Addr};
use log::{error, info, warn};
use get_if_addrs::{get_if_addrs, IfAddr};
use crate::{init::IP_REGISTER, uniclip::IMAGE_CHUNKS};

#[derive(Serialize, Deserialize)]
pub enum UniPacket {
    DiscoverySignal,
    Text(TextPacket),
    ImageChunk(ImageChunkPacket)
}

#[derive(Serialize, Deserialize)]
pub struct TextPacket {
    pub text: Vec<u8>,
}

pub async fn share_clip_text(item: String) -> Result<(), Box<dyn Error>> {
    let socket = UdpSocket::bind("0.0.0.0:0")
        .await.expect("Text-share UDP socket failed");

    let packet = UniPacket::Text(TextPacket {
        text: item.as_bytes().to_vec(),
    });

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
                error!("Text-share send failed to {}", address);
                return Err(Box::new(e));
        }
    }

    Ok(())
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ImageChunkPacket {
    pub hash: String,
    pub chunk_index: u32,
    pub total_chunks: u32,
    pub width: usize,
    pub height: usize,
    pub chunk_data: Vec<u8>,
}

pub async fn share_clip_img(
    item: ImageData<'_>, 
    hash: String, 
) -> Result<(), Box<dyn Error>> {
    let socket = UdpSocket::bind("0.0.0.0:0")
        .await.expect("Img-share UDP socket failed");
    
    let chunk_size = 60_000;
    let total_chunks = (
        item.bytes.len() as f32 / chunk_size as f32
    ).ceil() as u32;

    let ip_register = IP_REGISTER.lock().await;

    for (index, chunk) in item.bytes.chunks(chunk_size).enumerate() {
        let packet = UniPacket::ImageChunk(ImageChunkPacket {
            hash: hash.clone(),
            chunk_index: index as u32,
            total_chunks,
            width: item.width,
            height: item.height,
            chunk_data: chunk.to_vec()
        });

        let message = match bincode::serialize(&packet) {
            Ok(msg) => msg,
            Err(e) => {
                error!("Serialization of img packet failed: {}", e);
                return Err(Box::new(e));
            }
        };
    
        for ip in ip_register.iter() {
            let address = format!("{}:26025", ip);
            if let Err(e) = socket
                .send_to(&message, &address).await {
                    error!("Img-share send failed to {}", address);
                    return Err(Box::new(e));
            }
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

    let mut selected_own_ip: Option<String> = None;

    for iface in interfaces {
        if iface.is_loopback() {
            continue;
        }

        if let std::net::IpAddr::V4(ip_own) = iface.ip() {
            let octets = ip_own.octets();

            let is_private = (
                octets[0] == 10 ||
                octets[0] == 192 && octets[1] == 168) ||
                (octets[0] == 172 && (16..=31).contains(&octets[1])
            );

            if is_private {
                if selected_own_ip.is_none() || (octets[0] == 192 && octets[1] == 168) {
                    selected_own_ip = Some(ip_own.to_string());
                }
            }
        }
    }

    if let Some(ref ip) = selected_own_ip {
        ip_list.push(ip.to_string());
        info!("Own ip sent: {}", ip);
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

pub async fn cleanup_stale_chunks() {
    loop {
        sleep(Duration::from_secs(60)).await;

        let mut image_chunks = IMAGE_CHUNKS.lock().await;
        let now = Instant::now();

        image_chunks.retain(|_, (_, _, _, _, timestamp)| {
            if now.duration_since(*timestamp) > Duration::from_secs(60) {
                warn!("Removing stale image chunks");
                false
            } else {
                true
            }
        });
    }
} 
