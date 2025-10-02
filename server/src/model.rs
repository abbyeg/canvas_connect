use serde::{Deserialize, Serialize};

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
#[serde(tag="t", content="d")]
pub enum ClientMsg {
    #[serde(rename="join")]
    Join { 
        room_id: String, 
        #[serde(default)] 
        since: u64, // last seen seq
    },
    #[serde(rename="strokemsg")]
    StrokeMsg { stroke: Stroke, seq: u64 }, // seq provides ordering per client
    #[serde(rename="presence")]
    Presence { x: f32, y: f32, tool: u8, hue: u16 },
    // #[serde(rename="ack")]
    // Ack { upto: u64 },
    // #[serde(rename="snapshotreq")]
    // SnapshotReq // TODO: Add snapshots
}

#[derive(Serialize, Clone)]
#[serde(tag="t", content="d")]
pub enum ServerMsg {
    #[serde(rename="debug")]
    Debug { port: u16 },
}
