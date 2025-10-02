use std::{net::SocketAddr, sync::Arc};

use axum::{
    extract::{ws::{Message, WebSocket, WebSocketUpgrade}, ConnectInfo, Path, State}, http::HeaderMap, response::IntoResponse, routing::{any, get}, Router
};
// split websocket stream into separate TX and RX branches
use futures_util::{sink::SinkExt, stream::StreamExt};
use rmp_serde::{encode, from_slice};
use tokio::{sync::broadcast, time::Instant, time::Duration};
use uuid::Uuid;

use crate::{model::ClientMsg, AppState};
use crate::model::ServerMsg;
use crate::room::Room;

pub async fn ws_handler(
    State(state): State<AppState>,
    ws: WebSocketUpgrade,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    // headers: HeaderMap,
) -> impl IntoResponse {
    let room_id = Uuid::new_v4();
    tracing::debug!("peer {addr} connected, room id is {room_id}");
    let room = state.rooms.get_or_create(room_id);
    ws.on_upgrade(move |socket| handle_socket(socket, addr, room))
}

pub async fn ws_handler_with_room_id(
    State(state): State<AppState>,
    Path(room_id): Path<Uuid>,
    ws: WebSocketUpgrade,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    // headers: HeaderMap,
) -> impl IntoResponse {
    tracing::debug!("peer {addr} connected, room id is {room_id}");
    let room = state.rooms.get_or_create(room_id);
    ws.on_upgrade(move |socket| handle_socket(socket, addr, room))
}

async fn handle_socket(socket: WebSocket, who: SocketAddr, room: Arc<Room>) { 
    let (mut ws_tx, mut ws_rx) = socket.split(); // send debug info to client 
    let debug = ServerMsg::Debug { port: who.port(), room_id: room.id };
    let debug_payload = serde_json::to_string(&debug).expect("serialize"); tracing::debug!("debug_payload: {:?}", debug_payload); if ws_tx.send(Message::Text(debug_payload.into())).await.is_err() { return; } 
    // let patch = snapshot_tile(&state).await;
    // if ws_tx.send(Message::Text(patch.into())).await.is_err() { return; } 
    let mut rx = room.tx.subscribe(); 
    let (direct_tx, mut direct_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    // ---- Writer ---- // 
    let writer = tokio::spawn(async move { 
        loop { 
            tokio::select! { // direct, per-connection messages (e.g., join snapshot) 
            Some(msg) = direct_rx.recv() => { 
                if ws_tx.send(Message::Text(msg.into())).await.is_err() { break; } 
            }
            
            // room-wide broadcast messages
            recv = rx.recv() => { 
                match recv { 
                    Ok(msg) => { 
                        if ws_tx.send(Message::Text(msg.into())).await.is_err() { break; }
                    } 
                    Err(broadcast::error::RecvError::Lagged(_)) => continue, Err(_) => break, }
                } 
            }
        }
    }); 
    
    // ---- Reader ---- //
    let reader = {
        let direct_tx = direct_tx.clone();
    
        tokio::spawn(async move { 
            const MAX_BYTES: usize = 64*1024; 
            while let Some(Ok(msg)) = ws_rx.next().await { 
                match msg { 
                    Message::Text(text) => { if text.len() > MAX_BYTES { continue; } 

                        match serde_json::from_str::<ClientMsg>(&text) { 
                            Ok(ClientMsg::Join { .. }) => { 
                                tracing::debug!("peer {who} joined");
                                // let patch = snapshot_tile(&state).await;
                                // let _ = direct_tx.send(patch); // send only to this socket
                            } 
                            Ok(ClientMsg::Dabs { tool, dabs }) => { 
                                if dabs.len() % 4 != 0 || dabs.len() > 4096 { continue; }
                                let echo = serde_json::to_string(&ClientMsg::Dabs{ tool, dabs: dabs.clone() }).unwrap();
                                let _ = room.tx.send(echo.into()); // TODO: fix this
                                // apply_dabs_to_tile(&state, tool, &dabs).await; 
                            }
                            Err(e) => tracing::debug!("Malformed json: {:?}", e), 
                        }
                    }
                    Message::Binary(_) => {},
                    Message::Close(_) => break, 
                    Message::Ping(_) | Message::Pong(_) => {} 
                }
            } 
        }) 
    }; 
    
    tokio::select! { _ = writer => {}, _ = reader => {} } 
}
