//! Minimal Control Panel for hfm-text-filter using eframe and egui.

use eframe::egui;
use eframe::egui::{CentralPanel, Ui};

struct ServerApp {
    is_running: bool,
    port: u16,
}

impl ServerApp {
    fn new() -> Self {
        Self {
            is_running: false,
            port: 8377, // Default port
        }
    }

    fn render_ui(&mut self, ui: &mut Ui) {
        CentralPanel::default().show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(20.0);
                ui.heading("hfm-text-filter Server");
                ui.add_space(10.0);

                // Static warning note
                ui.label("Note: Closing this window will shut down the server.");
                ui.add_space(20.0);

                // Status Display
                if self.is_running {
                    ui.colored_label(egui::Color32::GREEN, format!("Status: Server is running on port {}", self.port));
                } else {
                    ui.colored_label(egui::Color32::RED, "Status: Server is stopped");
                }
                ui.add_space(20.0);

                // Start/Stop Button
                let button_text = if self.is_running { "Stop Server" } else { "Start Server" };
                if ui.button(button_text).clicked() {
                    self.is_running = !self.is_running;
                    
                    // TODO: Wire this up to the Axum server thread
                    if self.is_running {
                        println!("Starting server on port {}", self.port);
                    } else {
                        println!("Stopping server...");
                    }
                }
            });
        });
    }
}

impl eframe::App for ServerApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
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