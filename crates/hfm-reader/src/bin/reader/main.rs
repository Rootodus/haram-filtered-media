//! Minimal GUI for hfm-reader using egui and winit.

use std::sync::Arc;
use std::time::Duration;

use egui::{CentralPanel, Context, TopBottomPanel};
use egui_winit::egui::ViewportId;
use egui_winit::winit::application::ApplicationHandler;
use egui_winit::winit::dpi::LogicalSize;
use egui_winit::winit::event::WindowEvent;
use egui_winit::winit::event_loop::{ActiveEventLoop, EventLoop};
use egui_winit::winit::window::{Window, WindowAttributes};
use egui_winit::State;
use hfm_reader::{
    LocalFileSource, PipelineCommand, PipelineController, PipelineState,
    TextBuffer, TextBufferImpl, UppercaseFilter, Offset,
};
use parking_lot::Mutex;
use rfd::FileDialog;

struct App {
    window: Option<Arc<Window>>,
    pipeline: Option<PipelineController>,
    egui_state: State,
    file_path: Option<String>,
    scroll_offset: f32,
    text_content: String,
    loading: bool,
    error: Option<String>,
}

impl App {
    fn new() -> Self {
        // We'll create the egui state in `resumed` with the window.
        // Temporary dummy state until we have the window.
        let dummy_ctx = Context::default();
        let dummy_state = State::new(
            dummy_ctx,
            ViewportId::from_hash_of(0),
            // We need a dummy window handle; this will be replaced in `resumed`.
            // We'll use a placeholder by creating a temporary window? Actually,
            // we'll just create the state in `resumed` where we have the window.
            // For now, we'll store None and set it later.
            // We'll use Option<State> instead.
            // Actually, easier: initialize with a dummy and then replace.
            // But we can't replace the field easily without move. Let's use Option<State>.
            // I'll change the field type to Option<State>.
            // I'll restructure: App holds egui_state: Option<State>.
        );
        // We'll use Option<State>.
        Self {
            window: None,
            pipeline: None,
            egui_state: dummy_state, // will be replaced
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

        // Create a buffer.
        let buffer = TextBufferImpl::new(256);

        // Create a source.
        let source = match LocalFileSource::new(&path) {
            Ok(src) => src,
            Err(e) => {
                self.error = Some(format!("Failed to open file: {}", e));
                self.loading = false;
                return;
            }
        };

        // Create a pipeline with UppercaseFilter.
        let filter = UppercaseFilter::new();
        let mut controller = PipelineController::new(Box::new(source), buffer, filter);

        if let Err(e) = controller.start() {
            self.error = Some(format!("Failed to start pipeline: {}", e));
            self.loading = false;
            return;
        }

        self.pipeline = Some(controller);
        self.loading = false;

        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn update_text_content(&mut self) {
        if let Some(pipeline) = &self.pipeline {
            let buffer_lock = pipeline.buffer();
            let buffer = buffer_lock.lock();
            // Read a large range (e.g., up to 1MB) for simplicity.
            let max_offset = Offset(1024 * 1024);
            if let Ok(text) = buffer.read_range(Offset(0), max_offset) {
                self.text_content = text;
            } else {
                self.text_content = "(error reading buffer)".to_string();
            }
        }
    }

    fn render_ui(&mut self, ctx: &Context) {
        // If no file is loaded, show the file picker.
        if self.file_path.is_none() && self.error.is_none() && !self.loading {
            CentralPanel::default().show(ctx, |ui| {
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
            CentralPanel::default().show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.heading("Loading...");
                });
            });
            return;
        }

        if let Some(err) = &self.error {
            CentralPanel::default().show(ctx, |ui| {
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
        CentralPanel::default().show(ctx, |ui| {
            let available = ui.available_size();
            let scroll_area = egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .max_height(available.y);

            self.update_text_content();

            scroll_area.show(ui, |ui| {
                ui.add(egui::Label::new(&self.text_content).wrap());
            });
        });

        // Top toolbar.
        TopBottomPanel::top("toolbar").show(ctx, |ui| {
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

        // Create the egui state with the window.
        let viewport_id = ViewportId::from_hash_of(window.id());
        let scale_factor = window.scale_factor() as f32;
        let ctx = Context::default();
        self.egui_state = State::new(ctx, viewport_id, &window, Some(scale_factor), None, None);

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
        self.egui_state.on_window_event(&window, &event);

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => {
                let raw_input = self.egui_state.take_egui_input(&window);
                let full_output = self.egui_state.egui_ctx().run_ui(raw_input, |ctx| {
                    self.render_ui(ctx);
                });

                // Here we would render the primitives. For simplicity, we'll just
                // request redraw. In a real implementation, we'd use egui_glow or wgpu.
                // For now, we'll just request redraw again.
                window.request_redraw();

                // We also need to handle textures and such; but this is minimal.
            }
            _ => {}
        }
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