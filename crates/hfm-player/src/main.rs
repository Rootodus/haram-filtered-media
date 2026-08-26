mod app;
mod audio_output;
mod config;
mod gst_source;
mod gui;
mod pipeline_manager;
mod pts_offset;
mod renderer;

use app::App;
use winit::event_loop::EventLoop;

#[cfg(feature = "dhat")]
use dhat::{Alloc, Profiler};

#[cfg(feature = "dhat")]
#[global_allocator]
static ALLOC: Alloc = Alloc;

#[cfg(not(feature = "dhat"))]
use mimalloc::MiMalloc;

#[cfg(not(feature = "dhat"))]
#[global_allocator]
static ALLOC: MiMalloc = MiMalloc;

fn main() {
    #[cfg(feature = "dhat")]
    let _dhat = Profiler::new_heap();

    let event_loop = EventLoop::new().unwrap();
    let mut app = App::new();
    event_loop.run_app(&mut app).unwrap();
}
