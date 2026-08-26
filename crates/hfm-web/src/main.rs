#![allow(unused)] // this is temporary, and perhaps lib.rs is also temporary

mod browser;
mod debug_config;
mod inference;
mod logging;
mod render;
mod shared_state;
mod tokenizer;
mod types;

use browser::{BrowserSession, capture_screenshot, extract_dom_nodes};
use debug_config::DebugConfig;
use inference::run_inference;
use render::{App, CustomAppEvent};
use shared_state::SharedAppState;
use tokenizer::{init_tokenizer, tokenize};
use types::{DomNode, FrameData, SEQ_LEN, VisualAction};

use anyhow::Result;
use ort::session::Session;
use ort::value::{DynValue, Value};
use renderdoc::{RenderDoc, V141};
use std::error::Error;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::unbounded_channel;
use winit::event_loop::EventLoop;

const TARGET_URL: &str = "https://en.wikipedia.org/wiki/HTML5";
const TARGET_SELECTOR: &str = "p";
const VIEWPORT_WIDTH: u32 = 1280;
const VIEWPORT_HEIGHT: u32 = 720;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    std::panic::set_hook(Box::new(|panic_info| {
        eprintln!("CRITICAL PANIC ENCOUNTERED: {}", panic_info);
        // The OS will clean up memory, and because we removed process::exit,
        // standard stack unwinding will still attempt to drop local variables!
    }));

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
                                "Warning: RenderDoc wrapper failed to initialize.Error: {:?}",
                                e
                            );
                        }
                    };
                }
                Err(e) => {
                    eprintln!(
                        "Warning: Could not load renderdoc.dll from disk.Error: {:?}",
                        e
                    );
                }
            }
        }
    }
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // Create shared state and ACK channel
    let (ack_tx, ack_rx) = tokio::sync::mpsc::channel::<()>(1);
    let state = Arc::new(SharedAppState::new(ack_tx));

    // Build the EventLoop with support for Custom App User Events
    let event_loop = EventLoop::<CustomAppEvent>::with_user_event().build()?;
    let proxy = event_loop.create_proxy();

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    // Shared reference to allow window destruction hooks to fire down the line
    let shutdown_tx_opt = Arc::new(Mutex::new(Some(shutdown_tx)));
    let shutdown_tx_ctrlc = shutdown_tx_opt.clone();

    tokio::spawn(async move {
        if let Ok(_) = tokio::signal::ctrl_c().await {
            println!("\n[Ctrl + C] Signal detected! Initiating instant shutdown pipeline...");
            let mut guard = shutdown_tx_ctrlc.lock().unwrap();
            if let Some(tx) = guard.take() {
                let _ = tx.send(());
            }
            let _ = proxy.send_event(CustomAppEvent::RequestShutdown);
        }
    });

    // Load ONNX model (must be done before inference)
    let _ = ort::init().commit();
    tokenizer::init_tokenizer("crates/hfm-web/tokenizer.json")?;

    let model_paths = vec!["crates/hfm-web/model.onnx"];
    let mut sessions = Vec::with_capacity(model_paths.len());
    for path in model_paths {
        let session = match Session::builder()?
            .with_intra_threads(1)?
            .with_inter_threads(1)?
            .with_execution_providers([ort::ep::DirectML::default().build()])?
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
                    .with_execution_providers([ort::ep::CPU::default().build()])?
                    .commit_from_file(path)?
            }
        };
        sessions.push(Arc::new(Mutex::new(session)));
    }

    // Create channels for async inference
    let (frame_tx, mut frame_rx) = unbounded_channel::<FrameData>();
    let (actions_tx, actions_rx) = unbounded_channel::<Vec<VisualAction>>();

    // Spawn inference task
    let sessions_for_inference = sessions.clone();
    tokio::spawn(async move {
        while let Some(frame_data) = frame_rx.recv().await {
            let (input_ids_vec, attention_mask_vec) = tokenize(&frame_data.text, SEQ_LEN);
            let ids_array = ndarray::Array2::from_shape_vec((1, SEQ_LEN), input_ids_vec)
                .expect("Failed to create input_ids array");
            let mask_array = ndarray::Array2::from_shape_vec((1, SEQ_LEN), attention_mask_vec)
                .expect("Failed to create attention_mask array");
            let ids_value = Value::from_array(ids_array)
                .expect("Failed to create input_ids Value")
                .into_dyn();
            let mask_value = Value::from_array(mask_array)
                .expect("Failed to create attention_mask Value")
                .into_dyn();

            // Run inference on all sessions (serial for simplicity)
            let mut all_actions = Vec::new();
            for session_arc in &sessions_for_inference {
                let mut session_guard = session_arc.lock().unwrap();
                if let Ok(actions) = run_inference(
                    &mut session_guard,
                    &ids_value,
                    &mask_value,
                    &frame_data.nodes,
                    frame_data.width as f32,
                    frame_data.height as f32,
                ) {
                    all_actions.extend(actions);
                }
            }
            if !all_actions.is_empty() {
                let _ = actions_tx.send(all_actions);
            }
        }
    });

    // Spawn the browser execution loop on its own OS thread.
    let state_for_browser = state.clone();
    let _browser_thread_handle = std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
        rt.block_on(async {
            if let Err(e) =
                run_browser_frame_loop(state_for_browser, ack_rx, shutdown_rx, frame_tx).await
            {
                eprintln!("Browser frame loop thread error: {}", e);
            }
        });
    });

    // Warmup
    const BATCH: usize = 1;
    let ids_array = ndarray::Array2::<i64>::zeros((BATCH, SEQ_LEN));
    let mask_array = ndarray::Array2::<i64>::zeros((BATCH, SEQ_LEN));
    let input_ids_value: DynValue = Value::from_array(ids_array)?.into_dyn();
    let attention_mask_value: DynValue = Value::from_array(mask_array)?.into_dyn();
    let input_ids_arc = Arc::new(input_ids_value);
    let attention_mask_arc = Arc::new(attention_mask_value);
    let empty_nodes: Vec<DomNode> = Vec::new();
    let dummy_w = 1.0;
    let dummy_h = 1.0;

    for _ in 0..5 {
        for session_arc in &sessions {
            let mut session_guard = session_arc.lock().unwrap();
            let _ = inference::run_inference(
                &mut session_guard,
                &input_ids_arc,
                &attention_mask_arc,
                &empty_nodes,
                dummy_w,
                dummy_h,
            );
        }
    }

    // Start the renderer
    let mut app = App::new(state, sessions, actions_rx);
    println!("Starting Window Event Loop...");

    let _run_result = event_loop.run_app(&mut app);

    // If the window was closed via the "X" button, fire the channel manually before exit
    let mut guard = shutdown_tx_opt.lock().unwrap();
    if let Some(tx) = guard.take() {
        let _ = tx.send(());
    }

    println!("Winit UI engine shut down.Awaiting background thread profile deletion...");

    // FORCE AN INSTANT OS TERMINATION
    // This cleanly cuts through any async channel blocks or frozen tokio background tasks
    println!("[Shutdown] Complete.Exiting terminal execution context.");
    std::process::exit(0);
}

async fn run_browser_frame_loop(
    state: Arc<SharedAppState>,
    mut ack_rx: tokio::sync::mpsc::Receiver<()>,
    mut shutdown_rx: tokio::sync::oneshot::Receiver<()>,
    frame_tx: tokio::sync::mpsc::UnboundedSender<FrameData>,
) -> Result<(), Box<dyn Error>> {
    let (browser, mut handler, profile_dir) = BrowserSession::launch().await?;

    tokio::spawn(async move {
        use futures::StreamExt;
        while let Some(_) = handler.next().await {}
    });

    println!("[LAUNCH TRACK] Requesting new page target via browser.new_page()...");
    let page = browser.new_page("about:blank").await?;
    println!("[LAUNCH TRACK] DevTools websocket handshake successful!");

    let mut session = BrowserSession {
        browser,
        page,
        profile_dir,
    };
    session
        .set_viewport(VIEWPORT_WIDTH, VIEWPORT_HEIGHT)
        .await?;
    session.navigate(TARGET_URL).await?;

    println!("Browser frame processing loop has started successfully.");

    loop {
        let nodes = extract_dom_nodes(&session.page, TARGET_SELECTOR).await?;
        let nodes: Vec<DomNode> = nodes
            .into_iter()
            .filter(|n| n.rect.width > 0.0 && n.rect.height > 0.0)
            .filter(|n| n.rect.y + n.rect.height <= 720.0)
            .collect();

        let (width, height, pixel_data) = capture_screenshot(&session.page).await?;
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let pixel_arc = Arc::from(pixel_data.into_boxed_slice());

        // Clone nodes for the frame; use the original for text extraction
        let nodes_for_frame = nodes.clone();
        state.update_frame(timestamp, width, height, nodes_for_frame, pixel_arc);

        tokio::select! {
            ack = ack_rx.recv() => {
                if ack.is_none() { break; }
                tokio::time::sleep(std::time::Duration::from_millis(16)).await;
            }
            _ = &mut shutdown_rx => {
                break;
            }
        }

        // Extract text and send for inference
        let mut text = String::new();
        for node in &nodes {
            if let Some(t) = &node.text {
                if !t.is_empty() {
                    text.push_str(t);
                    text.push(' ');
                }
            }
        }
        if text.is_empty() {
            text.push_str("empty");
        }

        let _ = frame_tx.send(FrameData {
            text,
            nodes,
            width,
            height,
        });
    }

    let _ = session.close_sync();
    println!("Browser frame processing loop completed cleanup successfully.");
    Ok(())
}

#[tokio::test]
async fn test_browser() {
    use futures::StreamExt;

    let (browser, mut handler, _profile_dir) =
        browser::session::BrowserSession::launch().await.unwrap();

    // Keep track of the background task handle so we can abort it cleanly at shutdown
    let handler_task = tokio::spawn(async move { while let Some(_) = handler.next().await {} });

    let page = browser.new_page("about:blank").await.unwrap();
    let mut session = browser::session::BrowserSession {
        browser,
        page,
        profile_dir: _profile_dir,
    };

    session.set_viewport(1280, 720).await.unwrap();
    session.navigate("https://example.com").await.unwrap();

    let nodes = browser::extract::extract_dom_nodes(&session.page, " p")
        .await
        .unwrap();
    println!("Nodes: {:?}", nodes);

    let (w, h, _pixels) = browser::screenshot::capture_screenshot(&session.page)
        .await
        .unwrap();
    println!("Screenshot: {}x{}", w, h);

    // Execute your refactored clean filesystem drop logic
    session.close_sync().unwrap();

    // Explicitly kill the detached tokio handler task so it doesn't log broken pipe errors
    handler_task.abort();
}
