//! Minimal GUI for hfm-reader using eframe and egui.

use std::sync::Arc;

use eframe::egui;
use eframe::egui::{CentralPanel, Panel, ScrollArea, Ui};
use hfm_reader::{
    LocalFileSource, PipelineCommand, PipelineController, PipelineState,
    TextBuffer, TextBufferImpl, UppercaseFilter, Offset,
};
use parking_lot::Mutex;
use rfd::FileDialog;

struct ReaderApp {
    controller: Option<PipelineController>,
    buffer: Arc<Mutex<TextBufferImpl>>,
    file_path: Option<String>,
    text_content: String,
    loading: bool,
    error: Option<String>,
}

impl ReaderApp {
    fn new() -> Self {
        Self {
            controller: None,
            buffer: Arc::new(Mutex::new(TextBufferImpl::new(256))),
            file_path: None,
            text_content: String::new(),
            loading: false,
            error: None,
        }
    }

    fn load_file(&mut self, path: String) {
        self.file_path = Some(path.clone());
        self.loading = true;
        self.error = None;

        // Create a source.
        let source = match LocalFileSource::new(&path) {
            Ok(src) => src,
            Err(e) => {
                self.error = Some(format!("Failed to open file: {}", e));
                self.loading = false;
                return;
            }
        };

        // Create a pipeline with UppercaseFilter (for demo; replace later with ONNX).
        let filter = UppercaseFilter::new();
        let mut controller = PipelineController::new(Box::new(source), self.buffer.clone(), filter);

        if let Err(e) = controller.start() {
            self.error = Some(format!("Failed to start pipeline: {}", e));
            self.loading = false;
            return;
        }

        self.controller = Some(controller);
        self.loading = false;
    }

    fn update_text_content(&mut self) {
        if let Some(controller) = &self.controller {
            let buffer_lock = controller.buffer();
            let buffer = buffer_lock.lock();
            // Read a large range (e.g., up to 1MB) for simplicity.
            let max_offset = Offset(1024 * 1024);
            if let Ok(text) = buffer.read_range(Offset(0), max_offset) {
                let text_len = text.len();
                self.text_content = text;
                // Optional: print buffer length occasionally to see if it grows.
                // We'll print only when the text is non-empty to reduce noise.
                if text_len > 0 {
                    println!("[UI] Buffer text length: {} chars", text_len);
                }
            } else {
                self.text_content = "(error reading buffer)".to_string();
                // Also print the error to console for debugging.
                if let Err(e) = buffer.read_range(Offset(0), max_offset) {
                    eprintln!("[UI] read_range error: {:?}", e);
                }
            }
        }
    }

    fn render_ui(&mut self, ui: &mut Ui) {
        // If no file is loaded, show the file picker.
        if self.file_path.is_none() && self.error.is_none() && !self.loading {
            CentralPanel::default().show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.heading("hfm-reader");
                    ui.add_space(20.0);
                    if ui.button("Open Text File").clicked() {
                        if let Some(path) = FileDialog::new()
                            .add_filter("Text files", &["txt", "md", "rs"])
                            .pick_file()
                        {
                            self.load_file(path.to_string_lossy().to_string());
                        }
                    }
                });
            });
            return;
        }

        if self.loading {
            CentralPanel::default().show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.heading("Loading...");
                });
            });
            return;
        }

        if let Some(err) = self.error.clone() {
            CentralPanel::default().show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.heading("Error");
                    ui.label(err);
                    if ui.button("Try Again").clicked() {
                        self.error = None;
                        self.file_path = None;
                    }
                });
            });
            return;
        }

        // Main text view.
        CentralPanel::default().show(ui, |ui| {
            let available = ui.available_size();
            let scroll_area = ScrollArea::vertical()
                .auto_shrink([false; 2])
                .max_height(available.y);

            self.update_text_content();

            scroll_area.show(ui, |ui| {
                ui.add(egui::Label::new(&self.text_content).wrap());
            });
        });

        // Top toolbar.
        Panel::top("toolbar").show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Open").clicked() {
                    if let Some(path) = FileDialog::new()
                        .add_filter("Text files", &["txt", "md", "rs"])
                        .pick_file()
                    {
                        self.load_file(path.to_string_lossy().to_string());
                    }
                }
                ui.label(format!("File: {}", self.file_path.as_deref().unwrap_or("none")));
                if let Some(controller) = &self.controller {
                    let state = controller.state();
                    ui.label(format!("State: {:?}", state));
                    if state == PipelineState::Running {
                        if ui.button("Pause").clicked() {
                            let _ = controller.send_command(PipelineCommand::Pause);
                        }
                    } else if state == PipelineState::Paused {
                        if ui.button("Resume").clicked() {
                            let _ = controller.send_command(PipelineCommand::Resume);
                        }
                    }
                    if ui.button("Stop").clicked() {
                        let _ = controller.send_command(PipelineCommand::Stop);
                    }
                }
            });
        });
    }
}

impl eframe::App for ReaderApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        // If we have a controller, ensure we repaint frequently to update text.
        if self.controller.is_some() {
            self.update_text_content();
            ui.ctx().request_repaint();
        }
        self.render_ui(ui);
    }
}

fn main() -> Result<(), eframe::Error> {
    env_logger::init();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([800.0, 600.0])
            .with_title("hfm-reader"),
        ..Default::default()
    };
    eframe::run_native(
        "hfm-reader",
        options,
        Box::new(|_cc| Ok(Box::new(ReaderApp::new()))),
    )
}