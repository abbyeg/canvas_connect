use crate::model::Stroke;
use tokio::sync::{broadcast, RwLock};
use dashmap::DashMap;
use uuid::Uuid;
use std::sync::Arc;

use crate::model::Tile;

#[derive(Debug)]
pub struct Room {
    pub id: Uuid,
    pub tx: broadcast::Sender<String>, // TODO: convert to bytes
    // pub strokes: RwLock<Vec<Stroke>>,
    pub version: RwLock<u64>,
    pub tile: RwLock<Tile>,
}

impl Room {
    pub fn new(id: Uuid) -> Self {
        let (tx, _rx) = broadcast::channel(1024);
        Self { 
            id, 
            tx, 
            /*strokes: RwLock::new(Vec::new()),*/ 
            version: RwLock::new(0),
            tile: RwLock::new(Tile::new(1024,1024)),
        }
    }
}

#[derive(Default, Debug)]
pub struct Rooms(pub DashMap<Uuid, Arc<Room>>);

impl Rooms {
    pub fn get_or_create(&self, id: Uuid) -> Arc<Room> {
        self.0.entry(id).or_insert_with(|| Arc::new(Room::new(id))).clone()
    }
}
