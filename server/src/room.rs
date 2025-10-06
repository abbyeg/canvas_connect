use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::{RwLock, broadcast};
use uuid::Uuid;

use crate::model::Tile;

#[derive(Debug)]
pub struct Room {
    pub id: Uuid,
    pub tx: broadcast::Sender<String>, // TODO: convert to bytes
    pub tile: RwLock<Tile>,
}

impl Room {
    pub fn new(id: Uuid) -> Self {
        let (tx, _rx) = broadcast::channel(1024);
        Self {
            id,
            tx,
            tile: RwLock::new(Tile::new(800, 800)),
        }
    }
}

#[derive(Default, Debug)]
pub struct Rooms(pub DashMap<Uuid, Arc<Room>>);

impl Rooms {
    pub fn get_or_create(&self, id: Uuid) -> Arc<Room> {
        self.0
            .entry(id)
            .or_insert_with(|| Arc::new(Room::new(id)))
            .clone()
    }
}
