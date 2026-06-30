use ml_filtered_browser::network::start_ipc_server;
use ml_filtered_browser::render::App;
use ml_filtered_browser::state::SharedAppState;

use ort::session::Session;
use std::error::Error;
use std::sync::{Arc, Mutex};
use winit::event_loop::EventLoop;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let (ack_tx, ack_rx) = tokio::sync::mpsc::channel::<()>(1);
    let state = Arc::new(SharedAppState::new(ack_tx));

    let state_for_ipc = state.clone();
    tokio::spawn(async move {
        let _ = start_ipc_server(state_for_ipc, ack_rx).await;
    });

    // Load large model
    let model_paths = vec!["model.onnx"];
    let mut sessions = Vec::with_capacity(model_paths.len());
    for path in model_paths {
        let session = Session::builder()?
            .with_profiling("onnx_profile")?
            .with_execution_providers([
                ort::execution_providers::CPUExecutionProvider::default().build()
            ])?
            .commit_from_file(path)?;
        sessions.push(Arc::new(Mutex::new(session)));
    }

    // Clone the sessions vector before moving it into App
    let sessions_for_profiling = sessions.clone();

    let event_loop = EventLoop::new()?;
    let mut app = App::new(state, sessions);
    println!("Starting Window Event Loop...");
    event_loop.run_app(&mut app)?;

    // End profiling for all sessions after the event loop exits
    for session_arc in &sessions_for_profiling {
        let mut session_guard = session_arc.lock().unwrap(); // mutable lock for end_profiling
        if let Ok(filename) = session_guard.end_profiling() {
            println!("Profiling data written to: {}", filename);
        } else {
            eprintln!("Failed to end profiling for a session");
        }
    }

    Ok(())
}
