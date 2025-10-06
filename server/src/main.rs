use axum::{
    Router,
    routing::{any},
};
use std::{net::SocketAddr, path::PathBuf, sync::Arc};
use tower_http::{
    services::ServeDir,
    trace::{DefaultMakeSpan, TraceLayer},
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

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
        .route("/ws", any(ws_handler))
        .route("/ws/{room_id}", any(ws_handler_with_room_id))
        .fallback_service(ServeDir::new(assets_dir).append_index_html_on_directories(true))
        // .route("/api/export/:room.png", get(export_png)) // TODO: implement at some point
        .with_state(AppState {
            rooms: Arc::new(Rooms::default()),
        })
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::default().include_headers(true)),
        );

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();

    tracing::debug!("listening on {}", listener.local_addr().unwrap());
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();
}
