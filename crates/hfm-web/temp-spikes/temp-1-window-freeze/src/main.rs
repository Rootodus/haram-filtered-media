use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

struct App {
    rx: mpsc::Receiver<()>,
    frame_count: u32,
    redraw_count: u32,
    window: Option<Window>,
}

impl ApplicationHandler for App {
    // Note: The public API defines these using `&ActiveEventLoop`
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let window_attributes = Window::default_attributes();
            // create_window takes &self according to the API signature
            let window = event_loop.create_window(window_attributes).unwrap();
            self.window = Some(window);
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::RedrawRequested => {
                self.redraw_count += 1;
                if self.redraw_count % 60 == 0 {
                    println!("[Render] Redraw #{}", self.redraw_count);
                }
            }
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Ok(()) = self.rx.try_recv() {
            self.frame_count += 1;

            if self.frame_count % 30 == 0 {
                let start = Instant::now();
                println!("[Sim] Inference start (blocking)");
                thread::sleep(Duration::from_millis(23));
                let elapsed = start.elapsed();
                println!("[Sim] Inference done in {:?}", elapsed);
            }

            if let Some(ref window) = self.window {
                window.request_redraw();
            }
        }

        event_loop.set_control_flow(ControlFlow::Poll);
    }
}

fn main() {
    // EventLoop::new returns a Result<(), EventLoopError>
    let event_loop = EventLoop::new().unwrap();

    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        let tick_duration = Duration::from_millis(16);
        loop {
            thread::sleep(tick_duration);
            if tx.send(()).is_err() {
                break;
            }
        }
    });

    let mut app = App {
        rx,
        frame_count: 0,
        redraw_count: 0,
        window: None,
    };

    // run_app handles the execution loop using the struct references
    event_loop.run_app(&mut app).unwrap();
}
