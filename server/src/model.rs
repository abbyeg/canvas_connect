use serde::{Deserialize, Serialize};
use tiny_skia::Pixmap;
use uuid::Uuid;


#[derive(Serialize, Deserialize, Clone)]
pub struct DabsPayload {
    pub tool: u8,
    pub dabs: Vec<f32>,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMsg {
    #[serde(rename = "join")]
    Join {
        #[serde(default)]
        since: u64, // last seen seq
    },
    #[serde(rename = "dabs")]
    Dabs(DabsPayload),
}

#[derive(Serialize, Clone)]
#[serde(tag = "type")]
pub enum ServerMsg {
    #[serde(rename = "debug")]
    Debug { port: u16, room_id: Uuid },
    #[serde(rename = "tile_patch")]
    TilePatch {
        tx: i32,
        ty: i32,
        version: u64,
        png_base64: String,
    },
    #[serde(rename = "dabs")]
    Dabs(DabsPayload),
}

#[derive(Debug)]
pub struct Tile {
    pub pix: Pixmap,
    pub version: u64,
}

impl Tile {
    pub fn new(w: u32, h: u32) -> Self {
        Self {
            pix: Pixmap::new(w, h).unwrap(),
            version: 0,
        }
    }
}
