use axum::{
    extract::{ws::{Message, WebSocket, WebSocketUpgrade}, ConnectInfo, State},
    response::IntoResponse, 
    routing::{any, get}, 
    Router
};
use tiny_skia::{BlendMode, Color, Paint, PathBuilder, Pixmap, Transform};
use tokio::sync::{broadcast, RwLock, mpsc};
use std::{io::Write, net::SocketAddr, ops::ControlFlow, path::PathBuf, sync::Arc, time::Duration};
use tower_http::{
    services::ServeDir,
    trace::{
        DefaultMakeSpan, TraceLayer
    }
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
// split websocket stream into separate TX and RX branches
use futures_util::{sink::SinkExt, stream::StreamExt};
use base64::Engine;

mod model;
mod room;
mod ws;

use room::Rooms;
use ws::{ws_handler, ws_handler_with_room_id};

#[derive(Clone, Debug)]
pub struct AppState {
    rooms: Arc<Rooms>,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                format!("{}=debug,tower_http=debug", env!("CARGO_CRATE_NAME")).into()
            }),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets");
    
    let app = Router::new()
        .fallback_service(ServeDir::new(assets_dir).append_index_html_on_directories(true))
        .route("/ws", any(ws_handler))
        .route("/ws/{room_id}", any(ws_handler_with_room_id))
        // .route("/ws/:room", get(ws_handler)) // TODO: By room
        // .route("/api/export/:room.png", get(export_png)) // TODO: implement at some point
        .with_state(AppState { rooms: Arc::new(Rooms::default()) })
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::default().include_headers(true))
        );

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .unwrap();

    tracing::debug!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await.unwrap();
}

