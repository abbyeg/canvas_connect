use axum::{
    extract::{ws::{Message, WebSocket, WebSocketUpgrade}, ConnectInfo, State},
    response::IntoResponse, 
    routing::any, 
    Router
};
use tokio::sync::broadcast;
use std::{net::SocketAddr, ops::ControlFlow, path::PathBuf, time::Duration};
use tower_http::{
    services::ServeDir,
    trace::{
        DefaultMakeSpan, TraceLayer
    }
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
// split websocket stream into separate TX and RX branches
use futures_util::{sink::SinkExt, stream::StreamExt};
use serde::{Serialize, Deserialize};

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
    
    let (tx, _rx) = broadcast::channel(1024);
    let app = Router::new()
        .fallback_service(ServeDir::new(assets_dir).append_index_html_on_directories(true))
        .route("/ws", any(ws_handler))
        .with_state(AppState { tx })
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

#[derive(Clone, Debug)]
struct AppState {
    tx: broadcast::Sender<String>,
}

#[derive(Deserialize)]
#[serde(tag="type")]
enum ClientMsg {
    #[serde(rename="join")]
    Join { 
        room_id: String, 
        #[serde(default)] 
        known: std::collections::HashMap<String, u64>
    },
    #[serde(rename="dabs")]
    Dabs { tool: u8, dabs: Vec<f32> }
}

#[derive(Serialize, Clone)]
#[serde(tag="type")]
enum ServerMsg {
    #[serde(rename="debug")]
    Debug { port: u16 },
    #[serde(rename="dabs")]
    Dabs { tool: u8, dabs: Vec<f32> }
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> impl IntoResponse {
    tracing::debug!("peer {addr} connected");
    ws.on_upgrade(move |socket| handle_socket(socket, addr, state))
}

async fn handle_socket(mut socket: WebSocket, who: SocketAddr, state: AppState) {
    tracing::debug!("Enter handle_socket with who: {:?}, state: {:?}", who, state);
    let (mut ws_tx, mut ws_rx) = socket.split();

    // send debug info to client
    let debug = ServerMsg::Debug { port: who.port() };
    let debug_payload = serde_json::to_string(&debug).expect("serialize");
    tracing::debug!("debug_payload: {:?}", debug_payload);
    if ws_tx.send(Message::Text(debug_payload.into())).await.is_err() { return; }
    
    // writer: forward broadcasts
    let mut rx = state.tx.subscribe();
    let writer = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            if ws_tx.send(Message::Text(msg.into())).await.is_err() {
                break;
            }
        }
    });

    // reader: validate, parse, and broadcast
    let reader = tokio::spawn(async move {
        const MAX_BYTES: usize = 64*1024;
        loop {
            let next = tokio::time::timeout(Duration::from_secs(60), ws_rx.next()).await;
            let Some(ev) = next.ok().flatten() else { break; };
            let Ok(msg) = ev else { break };
            match msg {
                Message::Text(text) => {
                    if text.len() > MAX_BYTES { continue; }
                    match serde_json::from_str::<ClientMsg>(&text) {
                        Ok(ClientMsg::Join { .. }) => {
                            tracing::debug!("peer {who} joined");
                        }
                        Ok(ClientMsg::Dabs { tool, dabs }) => {
                            if dabs.len() % 4 != 0 || dabs.len() > 4096 { continue; }
                            let out = serde_json::to_string(&ServerMsg::Dabs { tool, dabs }).unwrap();
                            let _ = state.tx.send(out);
                        }
                        Err(_) => {
                            // ignore malformed json
                        }
                    }
                }
                Message::Binary(_) => {
                    // ignore for now
                },
                Message::Close(_) => break,
                Message::Ping(_) | Message::Pong(_) => { /* Handled by tungstenite */ }
            }
        }
    });
    
    
    tokio::select! {
        _ = writer => {},
        _ = reader => {}
    }
    
    tracing::debug!("Websocket context {who} destroyed");
}

/// helper to print contents of messages to stdout. Has special treatment for Close.
fn process_message(msg: Message, who: SocketAddr) -> ControlFlow<(), ()> {
    match msg {
        Message::Text(t) => {
            println!(">>> {who} sent str: {t:?}");
        }
        Message::Binary(d) => {
            println!(">>> {who} sent {} bytes: {d:?}", d.len());
        }
        Message::Close(c) => {
            if let Some(cf) = c {
                println!(
                    ">>> {who} sent close with code {} and reason `{}`",
                    cf.code, cf.reason
                );
            } else {
                println!(">>> {who} somehow sent close message without CloseFrame");
            }
            return ControlFlow::Break(());
        }

        Message::Pong(v) => {
            println!(">>> {who} sent pong with {v:?}");
        }
        // You should never need to manually handle Message::Ping, as axum's websocket library
        // will do so for you automagically by replying with Pong and copying the v according to
        // spec. But if you need the contents of the pings you can see them here.
        Message::Ping(v) => {
            println!(">>> {who} sent ping with {v:?}");
        }
    }
    ControlFlow::Continue(())
}
