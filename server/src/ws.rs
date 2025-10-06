use std::{net::SocketAddr, sync::Arc};

use axum::{
    extract::{
        ConnectInfo, Path, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::IntoResponse,
};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use futures_util::{sink::SinkExt, stream::StreamExt};
use tiny_skia::{BlendMode, Color, Paint, PathBuilder, Transform};
use tokio::{sync::broadcast};
use uuid::Uuid;

use crate::model::ServerMsg;
use crate::room::Room;
use crate::{
    AppState,
    model::{ClientMsg},
};

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
    tracing::debug!("created room with id: {:?}", room.id);
    ws.on_upgrade(move |socket| handle_socket(socket, addr, room))
}

async fn handle_socket(socket: WebSocket, who: SocketAddr, room: Arc<Room>) {
    let (mut ws_tx, mut ws_rx) = socket.split(); // send debug info to client 

    // ---- Debug Payload ---- //
    let debug = ServerMsg::Debug {
        port: who.port(),
        room_id: room.id,
    };
    let debug_payload = serde_json::to_string(&debug).expect("serialize");
    tracing::debug!("debug_payload: {:?}", debug_payload);
    if ws_tx
        .send(Message::Text(debug_payload.into()))
        .await
        .is_err()
    {
        return;
    }

    let mut rx = room.tx.subscribe();
    let (direct_tx, mut direct_rx) = tokio::sync::mpsc::unbounded_channel::<String>();

    // ---- Writer ---- //
    let writer = tokio::spawn(async move {
        loop {
            tokio::select! {
                biased;
                // direct, per-connection messages
                Some(msg) = direct_rx.recv() => {
                    if ws_tx.send(Message::Text(msg.into())).await.is_err() { break; }
                }

                // room-wide broadcast messages
                recv = rx.recv() => {
                    match recv {
                        Ok(msg) => {
                            if ws_tx.send(Message::Text(msg.into())).await.is_err() { break; }
                        }
                        Err(broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(_) => break,
                    }
                }
            }
        }
    });

    // ---- Reader ---- //
    let reader = {
        let direct_tx = direct_tx.clone();

        tokio::spawn(async move {
            const MAX_BYTES: usize = 64 * 1024;
            while let Some(Ok(msg)) = ws_rx.next().await {
                match msg {
                    Message::Text(text) => {
                        if text.len() > MAX_BYTES {
                            continue;
                        }

                        match serde_json::from_str::<ClientMsg>(&text) {
                            Ok(ClientMsg::Join { .. }) => {
                                tracing::debug!("peer {who} joined");
                                let patch = snapshot_tile(&room).await;
                                let _ = direct_tx.send(patch); // send only to this socket
                            }
                            Ok(ClientMsg::Dabs(dabs_payload)) => {
                                if dabs_payload.dabs.len() % 4 != 0
                                    || dabs_payload.dabs.len() > 4096
                                {
                                    continue;
                                }
                                let echo =
                                    serde_json::to_string(&ServerMsg::Dabs(dabs_payload.clone()))
                                        .unwrap();
                                let _ = room.tx.send(echo.into()); // TODO: fix this
                                apply_dabs_to_tile(&room, dabs_payload.tool, &dabs_payload.dabs)
                                    .await;
                            }
                            Err(e) => tracing::debug!("Malformed json: {:?}", e),
                        }
                    }
                    Message::Binary(_) => {}
                    Message::Close(_) => break,
                    Message::Ping(_) | Message::Pong(_) => {}
                }
            }
        })
    };

    tokio::select! { _ = writer => {}, _ = reader => {} }
}

// TODO: move to separate tile/image rs file
async fn apply_dabs_to_tile(room: &Arc<Room>, tool: u8, dabs: &[f32]) {
    let mut tile = room.tile.write().await;
    let mut paint = Paint::default();
    paint.set_color(Color::from_rgba8(0, 0, 0, 255));
    paint.blend_mode = if tool == 1 {
        BlendMode::DestinationOut
    } else {
        BlendMode::SourceOver
    };
    for c in dabs.chunks_exact(4) {
        let (x, y, r, _a) = (c[0], c[1], c[2], c[3]);
        let path = PathBuilder::from_circle(x, y, r)
            .unwrap_or_else(|| PathBuilder::from_circle(x + 0.001, y + 0.001, r).unwrap());
        tile.pix.fill_path(
            &path,
            &paint,
            tiny_skia::FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
    tile.version += 1;
}

async fn snapshot_tile(room: &Arc<Room>) -> String {
    let tile = room.tile.read().await;
    let png = encode_png(tile.pix.data(), tile.pix.width(), tile.pix.height()).unwrap();
    serde_json::to_string(&ServerMsg::TilePatch {
        tx: 0,
        ty: 0,
        version: tile.version,
        png_base64: B64.encode(png),
    })
    .unwrap()
}

fn encode_png(pix_data: &[u8], tile_width: u32, tile_height: u32) -> anyhow::Result<Vec<u8>> {
    debug_assert_eq!(
        pix_data.len(),
        (tile_width as usize) * (tile_height as usize) * 4
    );

    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, tile_width, tile_height);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.set_compression(png::Compression::Fast);
    enc.set_filter(png::Filter::NoFilter);
    let mut writer = enc.write_header().unwrap();
    writer.write_image_data(pix_data)?;
    writer.finish()?;
    Ok(out)
}
