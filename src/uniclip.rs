use std::{borrow::Cow, error::Error};
use arboard::{Clipboard, ImageData};
use tokio::{
    time::{sleep, Duration},
    sync::Mutex, 
    task::JoinError
};
use lazy_static::lazy_static;
use log::{error, info};
use crate::unifunctions::{
    share_clip_text, 
    share_clip_img, 
    hash_img,
    ImagePacket
};

lazy_static! {
    pub static ref LAST_CLIP_TEXT: Mutex<String> = Mutex::new(String::new());
}

lazy_static! {
    pub static ref LAST_CLIP_IMG_HASH: Mutex<String> = Mutex::new(String::new());
}



pub async fn master_uniclip() {
    let clipboard_listen = tokio::spawn(
        listen_clipboard_changes()
    );

    if let Err(e) = clipboard_listen.await {
        log_task_error("master clip listen", e);
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

pub async fn handle_incoming_img(img_packet: ImagePacket) -> Result<(), Box<dyn Error>> {
    let mut last_clip_img_hash = LAST_CLIP_IMG_HASH.lock().await;

    if img_packet.hash != *last_clip_img_hash {
        info!("Clip img received");
        let mut clipboard = match Clipboard::new(){
            Ok(cb) => cb,
            Err(e) => {
                error!("Failed to open clipboard: {}", e);
                return Err(Box::new(e));
            }
        };
        *last_clip_img_hash = img_packet.hash.clone();
        let img = ImageData {
            width: img_packet.width,
            height: img_packet.height,
            bytes: Cow::from(img_packet.bytes),
        };
        if let Err(e) = clipboard.set_image(img.clone()) {
            error!("Incoming img clip set failed: {}", e);
            return Err(Box::new(e));
        }
    }

    Ok(())
}
