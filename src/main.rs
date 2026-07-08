use ml_filtered_browser::browser::{BrowserSession, capture_screenshot, extract_dom_nodes};
use ml_filtered_browser::debug_config::DebugConfig;
use ml_filtered_browser::protocol::SEQ_LEN;
use ml_filtered_browser::render::{App, CustomAppEvent};
use ml_filtered_browser::shared_state::SharedAppState;

use anyhow::Result;
use ort::session::Session;
use ort::value::{DynValue, Value};
use renderdoc::{RenderDoc, V141};
use std::error::Error;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use winit::event_loop::EventLoop;

const TARGET_URL: &str = "https://en.wikipedia.org/wiki/HTML5";
const TARGET_SELECTOR: &str = "p";
const VIEWPORT_WIDTH: u32 = 1280;
const VIEWPORT_HEIGHT: u32 = 720;

// Global atomic flag to communicate instant shutdown to our background OS thread
static RUNNING: AtomicBool = AtomicBool::new(true);

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
                            println!("RenderDoc DLL loaded and initialized - capture enabled.");
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

    // 1. Create shared state and ACK channel
    let (ack_tx, ack_rx) = tokio::sync::mpsc::channel::<()>(1);
    let state = Arc::new(SharedAppState::new(ack_tx));

    // 2. Build the EventLoop with explicit support for our Custom App User Events
    let event_loop = EventLoop::<CustomAppEvent>::with_user_event().build()?;
    let proxy = event_loop.create_proxy();

    // 3. Listen for Ctrl+C. The very first press triggers this instantly.
    tokio::spawn(async move {
        if let Ok(_) = tokio::signal::ctrl_c().await {
            println!("\n[Ctrl+C] Signal detected! Shutting down browser instantly...");

            // Toggle the loop variable so the background thread exits frame processing
            RUNNING.store(false, Ordering::SeqCst);

            // Wake up and force the blocking winit window event loop to drop out
            let _ = proxy.send_event(CustomAppEvent::RequestShutdown);
        }
    });

    // 4. LAUNCH BROWSER SYNCHRONOUSLY FIRST (Ensures clean printing order)
    let (mut session, mut handler) = BrowserSession::launch().await?;

    // Spawn only chromiumoxide's message pump handle to a background task
    tokio::spawn(async move {
        use futures::StreamExt;
        while let Some(_) = handler.next().await {}
    });

    session
        .set_viewport(VIEWPORT_WIDTH, VIEWPORT_HEIGHT)
        .await?;
    session.navigate(TARGET_URL).await?;
    println!("Browser successfully navigated to target URL.");

    // 5. Spawn your frame pipeline processing loop thread using the active session
    let state_for_browser = state.clone();
    let browser_thread = std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
        rt.block_on(async {
            if let Err(e) = run_browser_frame_loop(session, state_for_browser, ack_rx).await {
                eprintln!("Browser frame loop thread error: {}", e);
            }
        });
    });

    // 6. Load ONNX model
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

    // 7. Warmup
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

    // 8. Start the renderer
    let mut app = App::new(state, sessions);
    println!("Starting Window Event Loop...");

    // Run window system. When proxy fires RequestShutdown, this unblocks and drops below.
    let run_result = event_loop.run_app(&mut app);

    // Fallback toggle if window was closed via the "X" button instead of Ctrl+C
    RUNNING.store(false, Ordering::SeqCst);

    if let Err(e) = run_result {
        eprintln!("Application exited with error: {:?}", e);
        std::process::exit(1);
    }

    println!("Main application shut down cleanly. Exiting process.");
    Ok(())
}

/// The actual frame tracking task executed inside the background OS thread.
async fn run_browser_frame_loop(
    session: BrowserSession,
    state: Arc<SharedAppState>,
    mut ack_rx: tokio::sync::mpsc::Receiver<()>,
) -> Result<(), Box<dyn Error>> {
    println!("Browser frame processing loop has started successfully.");

    while RUNNING.load(Ordering::SeqCst) {
        // Extract DOM nodes
        let nodes = extract_dom_nodes(&session.page, TARGET_SELECTOR).await?;

        // Capture screenshot
        let (width, height, pixel_data) = capture_screenshot(&session.page).await?;

        // Create timestamp
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let pixel_arc = Arc::from(pixel_data.into_boxed_slice());

        // Update state
        state.update_frame(timestamp, width, height, nodes, pixel_arc);

        // Wait for the renderer ACK loop or exit early if a shutdown is signaled
        loop {
            if !RUNNING.load(Ordering::SeqCst) {
                break;
            }
            if let Ok(_) = ack_rx.try_recv() {
                break;
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }
    }

    println!("Cleaning up temporary user profile folders and shutting down Chrome process tree...");
    let _ = session.close().await?;
    println!("Browser session cleanup complete.");
    Ok(())
}

#[tokio::test]
async fn test_browser() {
    use ml_filtered_browser::browser;
    let (mut session, mut handler) = browser::session::BrowserSession::launch().await.unwrap();
    tokio::spawn(async move {
        use futures::StreamExt;
        while let Some(_) = handler.next().await {}
    });
    session.set_viewport(1280, 720).await.unwrap();
    session.navigate("https://example.com").await.unwrap();
    let nodes = browser::extract::extract_dom_nodes(&session.page, "p")
        .await
        .unwrap();
    println!("Nodes: {:?}", nodes);
    let (w, h, _pixels) = browser::screenshot::capture_screenshot(&session.page)
        .await
        .unwrap();
    println!("Screenshot: {}x{}", w, h);
    session.close().await.unwrap();
}
