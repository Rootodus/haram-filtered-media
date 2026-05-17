use ml_filtered_browser::network::start_ipc_server;
use ml_filtered_browser::render::App;
use ml_filtered_browser::state::SharedAppState;

use ort::session::Session;
use std::error::Error;
use std::sync::Arc;
use winit::event_loop::EventLoop;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let (ack_tx, ack_rx) = tokio::sync::mpsc::channel::<()>(1);
    let state = Arc::new(SharedAppState::new(ack_tx));

    let state_for_ipc = state.clone();
    tokio::spawn(async move {
        let _ = start_ipc_server(state_for_ipc, ack_rx).await;
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
