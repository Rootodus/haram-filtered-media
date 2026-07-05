use ml_filtered_browser::network::start_ipc_server;
use ml_filtered_browser::protocol::SEQ_LEN;
use ml_filtered_browser::render::App;
use ml_filtered_browser::shared_state::SharedAppState;

use anyhow::Result;
use ort::session::Session;
use ort::value::{DynValue, Value};
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

    let _ = ort::init().commit();

    ml_filtered_browser::tokenizer::init_tokenizer("tokenizer.json")?;

    // Load large model
    let model_paths = vec!["model.onnx"];
    let mut sessions = Vec::with_capacity(model_paths.len());

    for path in model_paths {
        // Attempt to create a session with DirectML explicitly
        let session = match Session::builder()?
            .with_intra_threads(1)?
            .with_inter_threads(1)?
            .with_execution_providers([
                ort::execution_providers::DirectMLExecutionProvider::default().build(),
            ])?
            .commit_from_file(path)
        {
            Ok(s) => {
                println!(
                    "SUCCESS: DirectML execution provider active for model: {}",
                    path
                );
                s
            }
            Err(e) => {
                eprintln!(
                    "DirectML failed to initialize: {}. Falling back to standard CPU...",
                    e
                );
                Session::builder()?
                    .with_intra_threads(1)?
                    .with_inter_threads(1)?
                    .with_execution_providers([
                        ort::execution_providers::CPUExecutionProvider::default().build(),
                    ])?
                    .commit_from_file(path)?
            }
        };
        sessions.push(Arc::new(Mutex::new(session)));
    }

    // Clone the sessions vector before moving it into App
    let sessions_for_profiling = sessions.clone();

    // Pre‑allocate tensors once (batch=1, seq_len=64)
    const BATCH: usize = 1;

    let ids_array = ndarray::Array2::<i64>::zeros((BATCH, SEQ_LEN));
    let mask_array = ndarray::Array2::<i64>::zeros((BATCH, SEQ_LEN));

    let input_ids_value: DynValue = Value::from_array(ids_array)?.into_dyn();
    let attention_mask_value: DynValue = Value::from_array(mask_array)?.into_dyn();

    let input_ids_arc = Arc::new(input_ids_value);
    let attention_mask_arc = Arc::new(attention_mask_value);

    // Warmup: compile shaders
    for _ in 0..5 {
        for session_arc in &sessions {
            let mut session_guard = session_arc.lock().unwrap();
            let _ = ml_filtered_browser::inference::run_inference_large(
                &mut session_guard,
                &input_ids_arc,
                &attention_mask_arc,
                None,
            );
        }
    }

    let event_loop = EventLoop::new()?;
    let mut app = App::new(state, sessions);
    println!("Starting Window Event Loop...");
    event_loop.run_app(&mut app)?;

    // End profiling for all sessions after the event loop exits
    for session_arc in &sessions_for_profiling {
        let mut session_guard = session_arc.lock().unwrap();
        if let Ok(filename) = session_guard.end_profiling() {
            println!("Profiling data written to: {}", filename);
        } else {
            eprintln!("Failed to end profiling for a session");
        }
    }

    Ok(())
}
