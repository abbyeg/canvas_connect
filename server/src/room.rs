use crate::model::Stroke;
use tokio::sync::{broadcast, RwLock};
use dashmap::DashMap;
use uuid::Uuid;
use std::sync::Arc;

#[derive(Debug)]
pub struct Room {
    pub tx: broadcast::Sender<Vec<u8>>,
    pub strokes: RwLock<Vec<Stroke>>,
    pub version: RwLock<u64>,
}

impl Room {
    pub fn new() -> Self {
        let (tx, _rx) = broadcast::channel(1024);
        Self { tx, strokes: RwLock::new(Vec::new()), version: RwLock::new(0) }
    }
}

#[derive(Default, Debug)]
pub struct Rooms(pub DashMap<Uuid, Arc<Room>>);

impl Rooms {
    pub fn get_or_create(&self, id: Uuid) -> Arc<Room> {
        self.0.entry(id).or_insert_with(|| Arc::new(Room::new())).clone()
    }
}
