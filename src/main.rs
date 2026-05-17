use ort::session::Session;
use std::error::Error;
use std::sync::Arc;
use tokio::net::TcpListener;
use winit::event_loop::EventLoop;

use ml_filtered_browser::network::handle_connection;
use ml_filtered_browser::protocol::SharedAppState;
use ml_filtered_browser::render::App;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let (ack_tx, ack_rx) = tokio::sync::mpsc::channel::<()>(1);
    let state = Arc::new(SharedAppState::new(ack_tx));

    let state_for_ipc = state.clone();
    tokio::spawn(async move {
        let addr = "127.0.0.1:8080";
        let listener = TcpListener::bind(addr)
            .await
            .expect("Failed to bind TCP listener");
        println!("Listening on {}...", addr);

        let mut rx_holder = Some(ack_rx);
        while let Ok((stream, _)) = listener.accept().await {
            if let Some(rx) = rx_holder.take() {
                let s_handle = state_for_ipc.clone();
                tokio::spawn(async move {
                    let _ = handle_connection(stream, s_handle, rx).await;
                });
            }
        }
    });

    let session = Session::builder()?
        .with_execution_providers([
            ort::execution_providers::CPUExecutionProvider::default().build()
        ])?
        .commit_from_file("dummy_model.onnx")?;

    let event_loop = EventLoop::new()?;
    let mut app = App::new(state, Some(session));
    println!("Starting Window Event Loop...");
    event_loop.run_app(&mut app)?;
    Ok(())
}
