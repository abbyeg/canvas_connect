use serde::{Deserialize, Serialize};
use uuid::Uuid;
use tiny_skia::Pixmap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pt { x: f32, y: f32 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stroke {
    pub pts: Vec<Pt>,
    pub width: f32,
    pub rgba: [u8; 4],
    pub layer: u16,
}

#[derive(Serialize, Deserialize)]
#[serde(tag="type")]
pub enum ClientMsg {
    #[serde(rename="join")]
    Join { 
        room_id: String, 
        #[serde(default)] 
        since: u64, // last seen seq
    },
    #[serde(rename="dabs")]
    Dabs { tool: u8, dabs: Vec<f32> }, // TODO: Might not want to have same data for client/server
    // #[serde(rename="strokemsg")]
    // StrokeMsg { stroke: Stroke, seq: u64 }, // seq provides ordering per client
    // #[serde(rename="presence")]
    // Presence { x: f32, y: f32, tool: u8, hue: u16 },
    // #[serde(rename="ack")]
    // Ack { upto: u64 },
    // #[serde(rename="snapshotreq")]
    // SnapshotReq // TODO: Add snapshots
}

#[derive(Serialize, Clone)]
#[serde(tag="type")]
pub enum ServerMsg {
    #[serde(rename="debug")]
    Debug { port: u16, room_id: Uuid },
    #[serde(rename="tile_patch")]
    TilePatch { version: u64, png_base64: String},
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
            version: 0
        }
    }
}