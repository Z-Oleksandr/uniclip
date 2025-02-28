use std::{borrow::Cow, error::Error, collections::HashMap};
use arboard::{Clipboard, ImageData};
use tokio::{
    time::{sleep, Duration, Instant},
    sync::Mutex, 
    task::JoinError
};
use lazy_static::lazy_static;
use log::{error, info};
use crate::unifunctions::{
    share_clip_text, 
    share_clip_img, 
    hash_img,
    ImageChunkPacket,
    cleanup_stale_chunks
};

lazy_static! {
    pub static ref LAST_CLIP_TEXT: Mutex<String> = Mutex::new(String::new());
}

lazy_static! {
    pub static ref LAST_CLIP_IMG_HASH: Mutex<String> = Mutex::new(String::new());
}

lazy_static! {
    pub static ref IMAGE_CHUNKS: Mutex<HashMap<String, (
        Vec<Option<Vec<u8>>>, 
        usize, 
        usize, 
        u32, 
        Instant
    )>> = Mutex::new(HashMap::new());
}


pub async fn master_uniclip() {
    let clipboard_listen = tokio::spawn(
        listen_clipboard_changes()
    );
    let chunk_cleaner = tokio::spawn(
        cleanup_stale_chunks()
    );

    let (res1, res2) = tokio::join!(clipboard_listen, chunk_cleaner);

    if let Err(e) = res1 {
        log_task_error("master clip listen", e);
    }

    if let Err(e) = res2 {
        log_task_error("cleanup stale chunks", e);
    }
}

fn log_task_error(task_name: &str, err: JoinError) {
    error!("{} task failed: {}", task_name, err);
}

async fn listen_clipboard_changes() {
    let mut clipboard = match Clipboard::new() {
        Ok(cb) => cb,
        Err(e) => {
            error!("Clipboard open fail: {}", e);
            return;
        }
    };

    loop {
        let mut last_clip_text = LAST_CLIP_TEXT.lock().await;
        let mut last_clip_img_hash = LAST_CLIP_IMG_HASH.lock().await;

        if let Ok(content) = clipboard.get_text() {
            if content != *last_clip_text {
                *last_clip_text = content.clone();
                if let Err(e) = share_clip_text(content)
                    .await {
                        error!("Clipboard text share failed: {}", e);
                    }
            }
        }

        if let Ok(content) = clipboard.get_image() {
            let current_hash = hash_img(&content);

            if last_clip_img_hash.is_empty() || *last_clip_img_hash != current_hash {
                *last_clip_img_hash = current_hash.clone();
                if let Err(e) = share_clip_img(content, current_hash)
                    .await {
                        error!("Clip img share failed: {}", e);
                    }
            }
        }
        sleep(Duration::from_secs(1)).await;
    }
}

pub async fn handle_incoming_txt(text: String) -> Result<(), Box<dyn Error>> {
    let mut last_clip_text = LAST_CLIP_TEXT.lock().await;

    if text != *last_clip_text {
        info!("Clip text received");
        let mut clipboard = match Clipboard::new() {
            Ok(cb) => cb,
            Err(e) => {
                error!("Open clipboard failed: {}", e);
                return Err(Box::new(e));
            }
        };
        *last_clip_text = text.clone();
        if let Err(e) = clipboard.set_text(&text){
            error!("Incoming txt clip set failed");
            return Err(Box::new(e));
        }
    }

    Ok(())
}

pub async fn handle_incoming_img_chunk(chunk_packet: ImageChunkPacket) -> Result<(), Box<dyn Error>> {
    let mut image_chunks = IMAGE_CHUNKS.lock().await;

    let entry = image_chunks.entry(
        chunk_packet.hash.clone()
    ).or_insert_with(|| {
        (vec![None; chunk_packet.total_chunks as usize], chunk_packet.width, chunk_packet.height, chunk_packet.total_chunks, Instant::now())
    });

    let (chunks, width, height, _, _) = entry;

    if chunk_packet.chunk_index as usize >= chunks.len() {
        error!("Received chunk out of bounds.");
        return Err(Box::new(
            std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid chunk index")
        ));
    }

    chunks[chunk_packet.chunk_index as usize] = Some(chunk_packet.chunk_data);

    if chunks.iter().all(|c| c.is_some()) {
        let complete_data: Vec<u8> = chunks.iter().filter_map(
            |c| c.clone()
        ).flatten().collect();

        let img_packet = ImageData {
            width: *width,
            height: *height,
            bytes: Cow::from(complete_data),
        };

        let mut last_clip_img_hash = LAST_CLIP_IMG_HASH.lock().await;

        if chunk_packet.hash != *last_clip_img_hash {
            let mut clipboard = match Clipboard::new(){
                Ok(cb) => cb,
                Err(e) => {
                    error!("Failed to open clipboard: {}", e);
                    return Err(Box::new(e));
                }
            };
            *last_clip_img_hash = chunk_packet.hash.clone();

            if let Err(e) = clipboard.set_image(img_packet.clone()) {
                error!("Incoming img clip set failed: {}", e);
                return Err(Box::new(e));
            }

            image_chunks.remove(&chunk_packet.hash);

            info!("Image successfully reconstructed and set to clipboard");
        }
    }
    Ok(())
}
