mod app;
mod audio_output;
mod config;
mod gst_source;
mod gui;
mod pipeline_manager;
mod pts_offset;
mod renderer;

use app::App;
use mimalloc::MiMalloc;
use winit::event_loop::EventLoop;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

fn main() {
    let event_loop = EventLoop::new().unwrap();
    let mut app = App::new();
    event_loop.run_app(&mut app).unwrap();
}
