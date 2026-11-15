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
    Stop,
}

/// Responses sent from the background thread back to the UI.
enum ServerStatus {
    LoadingModel,
    Running,
    Stopped,
    Error(String),
}

/// The strict state representation for the UI.
enum UiServerState {
    Stopped,
    Loading,
    Running,
    Error(String),
}

/// Groups the active connections to the background thread.
/// Only exists if the server is running or loading.
struct ServerConnection {
    cmd_tx: Sender<ServerCommand>,
    status_rx: Receiver<ServerStatus>,
    handle: thread::JoinHandle<()>,
}

struct ServerApp {
    state: UiServerState,
    port: u16,
    connection: Option<ServerConnection>,
}

impl ServerApp {
    fn new() -> Self {
        Self {
            state: UiServerState::Stopped,
            port: 8377,
            connection: None,
        }
    }

    fn resolve_paths() -> Result<(PathBuf, Vec<String>), String> {
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
        
        self.state = UiServerState::Loading;
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

                let rt = match tokio::runtime::Runtime::new() {
                    Ok(rt) => rt,
                    Err(e) => {
                        status_tx.send(ServerStatus::Error(format!("Tokio error: {}", e))).unwrap();
                        return;
                    }
                };

                rt.block_on(async {
                    let server_fut = run_server(state, port);
                    
                    let stop_fut = async {
                        loop {
                            if let Ok(ServerCommand::Stop) = cmd_rx.try_recv() {
                                break;
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

        self.connection = Some(ServerConnection {
            cmd_tx,
            status_rx,
            handle,
        });
    }

    fn stop_server(&mut self) {
        if let Some(conn) = self.connection.take() {
            let _ = conn.cmd_tx.send(ServerCommand::Stop);
            let _ = conn.handle.join();
        }
        self.state = UiServerState::Stopped;
    }

    fn poll_status(&mut self) {
        if let Some(conn) = &self.connection {
            // Process incoming status messages
            while let Ok(status) = conn.status_rx.try_recv() {
                match status {
                    ServerStatus::LoadingModel => self.state = UiServerState::Loading,
                    ServerStatus::Running => self.state = UiServerState::Running,
                    ServerStatus::Stopped => self.state = UiServerState::Stopped,
                    ServerStatus::Error(e) => self.state = UiServerState::Error(e),
                }
            }

            // If the thread has finished, cleanly join and reset the connection.
            // This catches cases where the server crashes or exits unexpectedly.
            if conn.handle.is_finished() {
                if let Some(conn) = self.connection.take() {
                    let _ = conn.handle.join();
                }
                if !matches!(self.state, UiServerState::Error(_)) {
                    self.state = UiServerState::Stopped;
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

                // Strict state matching
                match &self.state {
                    UiServerState::Stopped => {
                        ui.colored_label(egui::Color32::YELLOW, "Status: Server is stopped");
                        ui.add_space(20.0);
                        if ui.button("Start Server").clicked() {
                            self.start_server();
                        }
                    }
                    UiServerState::Loading => {
                        ui.colored_label(egui::Color32::YELLOW, "Status: Loading model... (this may take a few seconds)");
                        ui.add_space(20.0);
                        // No button while loading
                    }
                    UiServerState::Running => {
                        ui.colored_label(egui::Color32::GREEN, format!("Status: Server is running on port {}", self.port));
                        ui.add_space(20.0);
                        if ui.button("Stop Server").clicked() {
                            self.stop_server();
                        }
                    }
                    UiServerState::Error(e) => {
                        ui.colored_label(egui::Color32::RED, format!("Status: Error: {}", e));
                        ui.add_space(20.0);
                        if ui.button("Try Again").clicked() {
                            self.state = UiServerState::Stopped;
                        }
                    }
                }
            });
        });
    }
}

impl eframe::App for ServerApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        self.poll_status();
        
        // If the server is active (Loading or Running), request frequent repaints
        if matches!(self.state, UiServerState::Loading | UiServerState::Running) {
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