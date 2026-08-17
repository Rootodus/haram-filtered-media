mod gst_source;
mod renderer;

use gst_source::GstSource;
use hfm_core::ml::PeopleSegFilter;
use hfm_core::pipeline::{PipelineCommand, PipelineController, SeekDelta};
use renderer::Renderer;
use std::sync::Arc;
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowId},
};

struct App {
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    pipeline: Option<PipelineController>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            window: None,
            renderer: None,
            pipeline: None,
        }
    }
}

impl App {
    fn init_pipeline(&mut self) {
        let source = GstSource::new().expect("Failed to create GStreamer source");
        let model = PeopleSegFilter::new("models/pphumanseg.onnx").expect("Failed to load model");
        let mut pipeline = PipelineController::new(Box::new(source), model);
        pipeline.start();
        self.pipeline = Some(pipeline);
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = Arc::new(
            event_loop
                .create_window(
                    winit::window::WindowAttributes::default()
                        .with_title("Audio+Video Pipeline")
                        .with_inner_size(winit::dpi::LogicalSize::new(960, 540)),
                )
                .unwrap(),
        );
        self.window = Some(window.clone());

        let renderer = pollster::block_on(Renderer::new(window));
        self.renderer = Some(renderer);

        self.init_pipeline();

        self.window.as_ref().unwrap().request_redraw();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => {
                let renderer = self.renderer.as_mut().unwrap();
                let frame = self
                    .pipeline
                    .as_ref()
                    .unwrap()
                    .pop_processed_frame()
                    .map(|f| f.data);
                renderer.render(frame);
                self.window.as_ref().unwrap().request_redraw();
            }
            WindowEvent::Resized(size) => {
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.resize(size.width, size.height);
                }
            }
            WindowEvent::KeyboardInput {
                event:
                    winit::event::KeyEvent {
                        logical_key: winit::keyboard::Key::Named(named_key),
                        state: winit::event::ElementState::Pressed,
                        ..
                    },
                ..
            } => {
                const SEEK_DELTA_NS: i64 = 10_000_000_000;
                if let Some(pipeline) = self.pipeline.as_ref() {
                    match named_key {
                        winit::keyboard::NamedKey::ArrowLeft => {
                            let _ = pipeline.send_command(PipelineCommand::Seek(
                                SeekDelta::Backward(SEEK_DELTA_NS as u64),
                            ));
                        }
                        winit::keyboard::NamedKey::ArrowRight => {
                            let _ = pipeline.send_command(PipelineCommand::Seek(
                                SeekDelta::Forward(SEEK_DELTA_NS as u64),
                            ));
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }
}

fn main() {
    let event_loop = EventLoop::new().unwrap();
    let mut app = App::default();
    event_loop.run_app(&mut app).unwrap();
}
