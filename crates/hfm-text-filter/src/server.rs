//! Axum WebSocket server for handling browser extension requests.

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
    routing::get,
    Router,
};
use futures_util::{sink::SinkExt, stream::StreamExt};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::filter::BartDetoxFilter;
use crate::types::RawChunk;

/// Shared state across all WebSocket connections.
#[derive(Clone)]
pub struct AppState {
    // We use parking_lot::Mutex because we are inside spawn_blocking.
    // std::sync::Mutex would also work, but parking_lot is faster.
    pub filter: Arc<Mutex<BartDetoxFilter>>,
}

#[derive(Deserialize)]
struct FilterRequest {
    text: String,
}

#[derive(Serialize)]
struct FilterResponse {
    clean_text: Option<String>,
    error: Option<String>,
}

/// Start the Axum server on the given port.
pub async fn run_server(state: AppState, port: u16) -> Result<(), String> {
    let app = Router::new()
        .route("/ws", get(ws_handler))
        .route("/health", get(|| async { "OK" }))
        .with_state(state);

    let addr = format!("127.0.0.1:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(|e| format!("Failed to bind to {}: {}", addr, e))?;

    println!("[SERVER] Listening on ws://{}/ws", addr);

    axum::serve(listener, app)
        .await
        .map_err(|e| format!("Server error: {}", e))
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();

    while let Some(Ok(msg)) = receiver.next().await {
        if let Message::Text(text) = msg {
            // 1. Parse the JSON
            let req: FilterRequest = match serde_json::from_str(&text) {
                Ok(r) => r,
                Err(e) => {
                    let resp = FilterResponse { clean_text: None, error: Some(format!("Invalid JSON: {}", e)) };
                    let _ = sender.send(Message::Text(serde_json::to_string(&resp).unwrap())).await;
                    continue;
                }
            };

            // 2. Clone the Arc and move to spawn_blocking
            let filter_arc = state.filter.clone();
            
            // 3. Run inference (blocking) in a separate thread
            let result = tokio::task::spawn_blocking(move || {
                let mut filter = filter_arc.lock();
                let raw = RawChunk::new(req.text);
                filter.process(&raw)
            }).await;

            // 4. Send response back to the client
            let resp = match result {
                Ok(Ok(processed)) => FilterResponse { clean_text: Some(processed.data), error: None },
                Ok(Err(e)) => FilterResponse { clean_text: None, error: Some(e.to_string()) },
                Err(e) => FilterResponse { clean_text: None, error: Some(format!("Task panic: {}", e)) },
            };

            let json = serde_json::to_string(&resp).unwrap();
            if sender.send(Message::Text(json)).await.is_err() {
                break; // Client disconnected
            }
        }
    }
}