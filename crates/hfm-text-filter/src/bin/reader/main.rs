//! Control Panel for hfm-text-filter using eframe and egui.

use std::path::PathBuf;
use std::sync::Arc;
use std::thread;

use crossbeam_channel::{bounded, Receiver, Sender};
use eframe::egui::{self, CentralPanel, Ui};
use parking_lot::Mutex;

use hfm_text_filter::filter::BartDetoxFilter;
use hfm_text_filter::server::{run_server, AppState};

/// Commands sent from the UI to the background server thread.
enum ServerCommand {
    Start,
    Stop,
}

/// Responses sent from the background thread back to the UI.
enum ServerStatus {
    LoadingModel,
    Running,
    Stopped,
    Error(String),
}

struct ServerApp {
    is_running: bool,
    port: u16,
    status_message: String,
    status_rx: Option<Receiver<ServerStatus>>,
    cmd_tx: Option<Sender<ServerCommand>>,
    server_thread: Option<thread::JoinHandle<()>>,
}

impl ServerApp {
    fn new() -> Self {
        Self {
            is_running: false,
            port: 8377,
            status_message: "Status: Server is stopped".to_string(),
            status_rx: None,
            cmd_tx: None,
            server_thread: None,
        }
    }

    fn resolve_paths() -> Result<(PathBuf, Vec<String>), String> {
        // 1. Check relative to executable (dist)
        // 2. Check current working directory (dev)
        // 3. Fallback to crate root (dev)
        
        let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.to_path_buf()));
        let cwd = std::env::current_dir().ok();
        let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

        let candidates = [exe_dir, cwd, Some(crate_root)].into_iter().flatten();

        let mut model_dir = None;
        let mut banned_words_path = None;

        for base in candidates {
            let m_path = base.join("models").join("bart-base-detox").join("onnx");
            let b_path = base.join("banned_words.txt");

            if m_path.exists() && b_path.exists() {
                model_dir = Some(m_path);
                banned_words_path = Some(b_path);
                break;
            }
        }

        let model_dir = model_dir.ok_or_else(|| "Could not find 'models/bart-base-detox/onnx'".to_string())?;
        let banned_path = banned_words_path.ok_or_else(|| "Could not find 'banned_words.txt'".to_string())?;

        let content = std::fs::read_to_string(&banned_path)
            .map_err(|e| format!("Failed to read banned_words.txt: {}", e))?;
            
        // Parse lines, stripping comments (lines starting with #)
        let words: Vec<String> = content
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(|l| l.to_string())
            .collect();

        if words.is_empty() {
            return Err("banned_words.txt is empty".to_string());
        }

        Ok((model_dir, words))
    }

    fn start_server(&mut self) {
        let (status_tx, status_rx) = bounded(16);
        let (cmd_tx, cmd_rx) = bounded(16);
        
        self.status_rx = Some(status_rx);
        self.cmd_tx = Some(cmd_tx);
        self.is_running = true;

        let port = self.port;
        
        let handle = thread::Builder::new()
            .name("server-manager".to_string())
            .spawn(move || {
                status_tx.send(ServerStatus::LoadingModel).unwrap();
                
                let (model_dir, words) = match Self::resolve_paths() {
                    Ok(res) => res,
                    Err(e) => {
                        status_tx.send(ServerStatus::Error(e)).unwrap();
                        return;
                    }
                };

                let filter = match BartDetoxFilter::new(&model_dir, words, 128, 2, 2) {
                    Ok(f) => f,
                    Err(e) => {
                        status_tx.send(ServerStatus::Error(format!("Failed to load model: {}", e))).unwrap();
                        return;
                    }
                };

                let state = AppState {
                    filter: Arc::new(Mutex::new(filter)),
                };

                // Create Tokio Runtime
                let rt = match tokio::runtime::Runtime::new() {
                    Ok(rt) => rt,
                    Err(e) => {
                        status_tx.send(ServerStatus::Error(format!("Tokio error: {}", e))).unwrap();
                        return;
                    }
                };

                rt.block_on(async {
                    // Start the server task
                    let server_fut = run_server(state, port);
                    
                    // Wait for the Stop command
                    let cmd_rx = cmd_rx;
                    let stop_fut = async {
                        loop {
                            // Poll the channel without blocking the Tokio runtime
                            if let Ok(cmd) = cmd_rx.try_recv() {
                                if let ServerCommand::Stop = cmd {
                                    break;
                                }
                            }
                            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                        }
                    };

                    tokio::select! {
                        res = server_fut => {
                            if let Err(e) = res {
                                status_tx.send(ServerStatus::Error(e)).unwrap();
                            }
                        }
                        _ = stop_fut => {
                            // Stopped by user
                        }
                    }
                });

                status_tx.send(ServerStatus::Stopped).unwrap();
            })
            .expect("Failed to spawn server manager thread");

        self.server_thread = Some(handle);
    }

    fn stop_server(&mut self) {
        if let Some(tx) = &self.cmd_tx {
            let _ = tx.send(ServerCommand::Stop);
        }
        if let Some(handle) = self.server_thread.take() {
            let _ = handle.join();
        }
        self.is_running = false;
        self.status_message = "Status: Server is stopped".to_string();
    }

    fn poll_status(&mut self) {
        if let Some(rx) = &self.status_rx {
            while let Ok(status) = rx.try_recv() {
                match status {
                    ServerStatus::LoadingModel => {
                        self.status_message = "Status: Loading model... (this may take a few seconds)".to_string();
                    }
                    ServerStatus::Running => {
                        self.status_message = format!("Status: Server is running on port {}", self.port);
                    }
                    ServerStatus::Stopped => {
                        self.is_running = false;
                        self.status_message = "Status: Server is stopped".to_string();
                    }
                    ServerStatus::Error(e) => {
                        self.is_running = false;
                        self.status_message = format!("Status: Error: {}", e);
                    }
                }
            }
        }
    }

    fn render_ui(&mut self, ui: &mut Ui) {
        CentralPanel::default().show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(20.0);
                ui.heading("hfm-text-filter Server");
                ui.add_space(10.0);

                ui.label("Note: Closing this window will shut down the server.");
                ui.add_space(20.0);

                let color = if self.is_running && self.status_message.contains("running") {
                    egui::Color32::GREEN
                } else if self.status_message.contains("Error") {
                    egui::Color32::RED
                } else {
                    egui::Color32::YELLOW
                };
                
                ui.colored_label(color, &self.status_message);
                ui.add_space(20.0);

                if self.is_running {
                    if ui.button("Stop Server").clicked() {
                        self.stop_server();
                    }
                } else {
                    if ui.button("Start Server").clicked() {
                        self.start_server();
                    }
                }
            });
        });
    }
}

impl eframe::App for ServerApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        self.poll_status();
        
        // If the server thread is running, request frequent repaints to update status
        if self.is_running {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
        }

        self.render_ui(ui);
    }
}

fn main() -> Result<(), eframe::Error> {
    env_logger::init();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([400.0, 300.0])
            .with_title("hfm-text-filter Server"),
        ..Default::default()
    };
    eframe::run_native(
        "hfm-text-filter",
        options,
        Box::new(|_cc| Ok(Box::new(ServerApp::new()))),
    )
}