use ml_filtered_browser::debug_config::DebugConfig;
use ml_filtered_browser::network::start_ipc_server;
use ml_filtered_browser::protocol::SEQ_LEN;
use ml_filtered_browser::render::App;
use ml_filtered_browser::shared_state::SharedAppState;

use anyhow::Result;
use ort::session::Session;
use ort::value::{DynValue, Value};
use renderdoc::{RenderDoc, V141};
use std::error::Error;
use std::sync::{Arc, Mutex};
use winit::event_loop::EventLoop;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let debug = DebugConfig::init();
    if debug.renderdoc_capture {
        unsafe {
            match libloading::Library::new(r"C:\Program Files\RenderDoc\renderdoc.dll") {
                Ok(lib) => {
                    std::mem::forget(lib);
                    println!("RenderDoc DLL loaded via libloading.");

                    match RenderDoc::<V141>::new() {
                        Ok(_rd) => {
                            println!("RenderDoc DLL loaded and initialized – capture enabled.");
                        }
                        Err(e) => {
                            eprintln!(
                                "Warning: RenderDoc wrapper failed to initialize. Error: {:?}",
                                e
                            );
                        }
                    };
                }
                Err(e) => {
                    eprintln!(
                        "Warning: Could not load renderdoc.dll from disk. Error: {:?}",
                        e
                    );
                }
            }
        }
    }
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // 1. Start network and shared state immediately
    let (ack_tx, ack_rx) = tokio::sync::mpsc::channel::<()>(1);
    let state = Arc::new(SharedAppState::new(ack_tx));

    let state_for_ipc = state.clone();
    tokio::spawn(async move {
        let _ = start_ipc_server(state_for_ipc, ack_rx).await;
    });

    // 2. Initialize winit EventLoop immediately (NO DirectML activity yet!)
    let event_loop = EventLoop::new()?;

    // 3. Load DirectML and the models now
    let _ = ort::init().commit();
    ml_filtered_browser::tokenizer::init_tokenizer("tokenizer.json")?;

    let model_paths = vec!["model.onnx"];
    let mut sessions = Vec::with_capacity(model_paths.len());

    for path in model_paths {
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
                    "DirectML failed to initialize: {}. Falling back to CPU...",
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

    let sessions_for_profiling = sessions.clone();

    // 4. Execute warmup (Allocate and run Tensors)
    const BATCH: usize = 1;
    let ids_array = ndarray::Array2::<i64>::zeros((BATCH, SEQ_LEN));
    let mask_array = ndarray::Array2::<i64>::zeros((BATCH, SEQ_LEN));

    let input_ids_value: DynValue = Value::from_array(ids_array)?.into_dyn();
    let attention_mask_value: DynValue = Value::from_array(mask_array)?.into_dyn();

    let input_ids_arc = Arc::new(input_ids_value);
    let attention_mask_arc = Arc::new(attention_mask_value);

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

    // 5. Create the application and start the event loop
    // TIP: Ensure that `instance.create_surface` is invoked first thing inside `App::resumed`!
    let mut app = App::new(state, sessions);
    println!("Starting Window Event Loop...");
    event_loop.run_app(&mut app)?;

    // 6. Finalize profiling after closing the window
    if DebugConfig::get().inference_profiling {
        for session_arc in &sessions_for_profiling {
            let mut session_guard = session_arc.lock().unwrap();
            if let Ok(filename) = session_guard.end_profiling() {
                println!("Profiling data written to: {}", filename);
            } else {
                eprintln!("Failed to end profiling for a session");
            }
        }
    }

    Ok(())
}
