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
    Path(room_id): Path<Uuid>,
    ws: WebSocketUpgrade,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    // headers: HeaderMap,
) -> impl IntoResponse {
    tracing::debug!("peer {addr} connected");
    let room = state.rooms.get_or_create(room_id);
    ws.on_upgrade(move |socket| handle_socket(socket, addr, room))
}


async fn handle_socket(socket: WebSocket, who: SocketAddr, room: Arc<Room>) {
    let (mut ws_tx, mut ws_rx) = socket.split();

    // send debug info to client
    let debug = ServerMsg::Debug { port: who.port() };
    let debug_payload = serde_json::to_string(&debug).expect("serialize");
    if ws_tx.send(Message::Text(debug_payload.into())).await.is_err() { return; }

    // send room data to this client
    let mut rx = room.tx.subscribe();
    let send_task = tokio::spawn(async move {
        while let Ok(bin) = rx.recv().await {
            if ws_tx.send(Message::Binary(bin.into())).await.is_err() { break; }
        }
    });

    let mut tokens = 20u32;
    let refill_every = Duration::from_millis(200);
    let mut last = Instant::now();
    
    while let Some(Ok(msg)) = ws_rx.next().await {
        if last.elapsed() >= refill_every {
            tokens = tokens.saturating_add(5).min(20);
            last = Instant::now();
        }
        if tokens == 0 { continue; } else { tokens -= 1; }

        if let Message::Binary(b) = msg {
            match from_slice::<ClientMsg>(&b) {
                Ok(ClientMsg::Join { room_id: _, since }) => {
                    let strokes = room.strokes.read().await.clone();
                    
                    for (i, s) in strokes.into_iter().enumerate() {
                        let client_stroke = ClientMsg::StrokeMsg { seq: i as u64 + 1, stroke: s };
                        let mut buf = Vec::with_capacity(256);
                        encode::write(&mut buf, &client_stroke).unwrap();
                        
                        if ws_tx.send(Message::Binary(buf.into())).await.is_err() { break; }
                    }
                }
                Ok(ClientMsg::StrokeMsg { seq: _, stroke }) => {
                    {
                        let mut v = room.strokes.write().await;
                        v.push(stroke.clone());
                        let mut ver = room.version.write().await;
                        *ver += 1;
                    }
                    let out = ClientMsg::StrokeMsg { seq: *room.version.read().await, stroke };
                    let mut buf = Vec::with_capacity(256);
                    encode::write(&mut buf, &out).unwrap();
                    let _ = room.tx.send(buf);
                },
                Ok(ClientMsg::Presence { .. }) => {
                    let _ = room.tx.send(b.to_vec());
                },
                Err(_) => { /* TODO: Log it */ }
            }
        }
    }

    send_task.abort();
}
