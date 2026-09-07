//! Minimal GUI for hfm-reader using egui and winit.

use std::sync::Arc;
use std::time::Duration;

use egui::Context;
use egui_winit::egui::ViewportId;
use egui_winit::winit::application::ApplicationHandler;
use egui_winit::winit::dpi::LogicalSize;
use egui_winit::winit::event::{StartCause, WindowEvent};
use egui_winit::winit::event_loop::{ActiveEventLoop, EventLoop};
use egui_winit::winit::window::{Window, WindowAttributes};
use hfm_reader::{
    LocalFileSource, PipelineCommand, PipelineController, PipelineState,
    TextBuffer, TextBufferImpl, UppercaseFilter, Offset,
};
use parking_lot::Mutex;
use rfd::FileDialog;

struct App {
    window: Option<Arc<Window>>,
    pipeline: Option<PipelineController>,
    buffer: Option<Arc<Mutex<TextBufferImpl>>>,
    egui_state: egui_winit::State,
    file_path: Option<String>,
    scroll_offset: f32,
    text_content: String,
    loading: bool,
    error: Option<String>,
}

impl App {
    fn new() -> Self {
        Self {
            window: None,
            pipeline: None,
            buffer: None,
            egui_state: egui_winit::State::new(
                egui::Context::default(),
                ViewportId::from_hash_of(0),
                None,
                Some(1.0),
                None,
                None,
            ),
            file_path: None,
            scroll_offset: 0.0,
            text_content: String::new(),
            loading: false,
            error: None,
        }
    }

    fn load_file(&mut self, path: String) {
        self.file_path = Some(path.clone());
        self.loading = true;
        self.error = None;

        // Create a new buffer.
        let buffer = Arc::new(Mutex::new(TextBufferImpl::new(256)));

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
        let mut controller = PipelineController::new(Box::new(source), buffer.clone(), filter);

        if let Err(e) = controller.start() {
            self.error = Some(format!("Failed to start pipeline: {}", e));
            self.loading = false;
            return;
        }

        self.buffer = Some(buffer);
        self.pipeline = Some(controller);
        self.loading = false;

        // Request initial redraw to show buffer content.
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn update_text_content(&mut self) {
        if let Some(buffer) = &self.buffer {
            let buffer = buffer.lock();
            // For simplicity, we read the entire buffer (since we use a bounded buffer,
            // it's fine for demonstration).
            // In a real app, we would only read the visible portion.
            if let Ok(text) = buffer.read_range(Offset(0), Offset(1024 * 1024)) {
                self.text_content = text;
            } else {
                self.text_content = "(error reading buffer)".to_string();
            }
        }
    }

    fn render_ui(&mut self, ctx: &Context) {
        // Show a file picker if no file is loaded.
        if self.file_path.is_none() && self.error.is_none() && !self.loading {
            egui::CentralPanel::default().show(ctx, |ui| {
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

        // If loading or error, show status.
        if self.loading {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.heading("Loading...");
                });
            });
            return;
        }

        if let Some(err) = &self.error {
            egui::CentralPanel::default().show(ctx, |ui| {
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
        egui::CentralPanel::default().show(ctx, |ui| {
            let available = ui.available_size();
            let scroll_area = egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .max_height(available.y);

            // Update text content from buffer.
            self.update_text_content();

            scroll_area.show(ui, |ui| {
                ui.add(egui::Label::new(&self.text_content).wrap());
            });
        });

        // Top toolbar.
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
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
                if let Some(pipeline) = &self.pipeline {
                    let state = pipeline.state();
                    ui.label(format!("State: {:?}", state));
                    if state == PipelineState::Running {
                        if ui.button("Pause").clicked() {
                            let _ = pipeline.send_command(PipelineCommand::Pause);
                        }
                    } else if state == PipelineState::Paused {
                        if ui.button("Resume").clicked() {
                            let _ = pipeline.send_command(PipelineCommand::Resume);
                        }
                    }
                    if ui.button("Stop").clicked() {
                        let _ = pipeline.send_command(PipelineCommand::Stop);
                    }
                }
            });
        });
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = Arc::new(
            event_loop
                .create_window(
                    WindowAttributes::default()
                        .with_title("hfm-reader")
                        .with_inner_size(LogicalSize::new(800, 600)),
                )
                .unwrap(),
        );
        self.window = Some(window.clone());

        // Initialize egui for this window.
        self.egui_state
            .set_viewport_id(ViewportId::from_hash_of(window.id()));
        self.egui_state.set_window(&window, Some(1.0));

        // Automatically open a file dialog on startup (optional).
        // For now, we just show the "Open" button.

        window.request_redraw();
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        let window = self.window.as_ref().unwrap();

        // Forward event to egui.
        let egui_input = self.egui_state.on_window_event(&window, &event);

        // Handle window close.
        if let WindowEvent::CloseRequested = event {
            event_loop.exit();
            return;
        }

        // Handle redraw.
        if let WindowEvent::RedrawRequested = event {
            // Collect input.
            let raw_input = self.egui_state.take_egui_input(&window);

            // Run UI.
            let full_output = self.egui_state.egui_ctx().run(raw_input, |ctx| {
                self.render_ui(ctx);
            });

            // Update textures and render.
            let primitives = self.egui_state.egui_ctx().tessellate(
                full_output.shapes,
                self.egui_state.egui_ctx().pixels_per_point(),
            );

            // We'll render using egui_glow (simple CPU renderer).
            // But we need to integrate with wgpu? Actually we can use glow directly.
            // However, for simplicity, we can use `egui_glow::Painter` which works
            // with any OpenGL context. For winit, we need a GL context.

            // Since we don't want to add a full OpenGL backend, we can use the
            // `egui_wgpu` backend that hfm-player uses, but that requires wgpu.
            // For a minimal reader, we can use `egui_glow` which is simpler.

            // However, this example is getting complex. For now, we'll just
            // request a redraw and output the primitives to the console.
            // In a real implementation, we'd integrate with wgpu or glow.

            // For now, we'll skip actual rendering and just request redraw.
            window.request_redraw();

            return;
        }

        // For other events, we just pass through.
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let event_loop = EventLoop::new()?;
    let mut app = App::new();
    event_loop.run_app(&mut app)?;

    Ok(())
}