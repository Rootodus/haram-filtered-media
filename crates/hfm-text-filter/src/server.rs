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

use crate::filter::TextFilter;
use crate::types::RawChunk;

/// Shared state across all WebSocket connections.
#[derive(Clone)]
pub struct AppState {
    pub filter: Arc<Mutex<Box<dyn TextFilter>>>,
}

#[derive(Serialize, Deserialize)]
struct FilterRequest {
    text: String,
}

#[derive(Serialize, Deserialize)]
struct FilterResponse {
    clean_text: Option<String>,
    error: Option<String>,
}

/// Start the Axum server on the given pre-bound listener.
pub async fn run_server(state: AppState, listener: tokio::net::TcpListener) -> Result<(), String> {
    let app = Router::new()
        .route("/ws", get(ws_handler))
        .route("/health", get(|| async { "OK" }))
        .with_state(state);

    let addr = listener.local_addr().map_err(|e| format!("Invalid listener: {}", e))?;
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
            let req: FilterRequest = match serde_json::from_str(&text.to_string()) {
                Ok(r) => r,
                Err(e) => {
                    let resp = FilterResponse { clean_text: None, error: Some(format!("Invalid JSON: {}", e)) };
                    let _ = sender.send(Message::Text(serde_json::to_string(&resp).unwrap().into())).await;
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
            if sender.send(Message::Text(json.into())).await.is_err() {
                break; // Client disconnected
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filter::UppercaseFilter;
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::connect_async;
    use tokio_tungstenite::tungstenite::Message as WsMessage;

    /// Spins up a test server on an ephemeral port with the UppercaseFilter.
    /// Returns the port to connect to.
    async fn spawn_test_server() -> u16 {
        let state = AppState {
            filter: Arc::new(Mutex::new(Box::new(UppercaseFilter::new()))),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        tokio::spawn(async move {
            let _ = run_server(state, listener).await;
        });

        // Give the listener a moment to start accepting before we connect.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        port
    }

    #[tokio::test]
    async fn test_websocket_uppercase() {
        let port = spawn_test_server().await;
        let url = format!("ws://127.0.0.1:{}/ws", port);
        let (mut ws, _) = connect_async(url).await.expect("Failed to connect");

        let req = FilterRequest {
            text: "hello world".to_string(),
        };
        let json = serde_json::to_string(&req).unwrap();
        ws.send(WsMessage::Text(json.into())).await.unwrap();

        let msg = ws.next().await.unwrap().unwrap();
        if let WsMessage::Text(text) = msg {
            let text_str = text.to_string();
            let resp: FilterResponse = serde_json::from_str(&text_str).unwrap();
            assert_eq!(resp.clean_text.unwrap(), "HELLO WORLD");
            assert!(resp.error.is_none());
        } else {
            panic!("Expected text message");
        }
    }

    #[tokio::test]
    async fn test_websocket_invalid_json() {
        let port = spawn_test_server().await;
        let url = format!("ws://127.0.0.1:{}/ws", port);
        let (mut ws, _) = connect_async(url).await.expect("Failed to connect");

        ws.send(WsMessage::Text("not json".into())).await.unwrap();

        let msg = ws.next().await.unwrap().unwrap();
        if let WsMessage::Text(text) = msg {
            let text_str = text.to_string();
            let resp: FilterResponse = serde_json::from_str(&text_str).unwrap();
            assert!(resp.clean_text.is_none());
            let err = resp.error.unwrap();
            assert!(
                err.contains("Invalid JSON"),
                "Expected error to mention 'Invalid JSON', got: {}",
                err
            );
        } else {
            panic!("Expected text message");
        }
    }
}
