use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Shared frame data: FlatBuffer bytes + raw pixel data
#[derive(Clone)] // Required for `lock.clone()`
pub struct FrameState {
    pub timestamp: u64,
    pub width: u32,
    pub height: u32,
    pub buffer: Arc<[u8]>, // FlatBuffer bytes
    pub pixel_data: Arc<[u8]>,
}

/// High-performance shared state
pub struct SharedAppState {
    pub frame: Mutex<Option<FrameState>>,
    pub dirty: AtomicBool,
    pub ack_sender: tokio::sync::mpsc::Sender<()>,
    pub clear_color: Mutex<wgpu::Color>,
    pub actions: Mutex<Vec<crate::protocol::VisualAction>>,
}

impl SharedAppState {
    pub fn new(ack_sender: tokio::sync::mpsc::Sender<()>) -> Self {
        Self {
            frame: Mutex::new(None),
            dirty: AtomicBool::new(false),
            ack_sender,
            clear_color: Mutex::new(wgpu::Color {
                r: 0.01,
                g: 0.01,
                b: 0.1,
                a: 1.0,
            }),
            actions: Mutex::new(Vec::new()),
        }
    }

    pub fn update_frame(
        &self,
        timestamp: u64,
        width: u32,
        height: u32,
        buffer: Arc<[u8]>,
        pixel_data: Arc<[u8]>,
    ) {
        let new_frame = FrameState {
            timestamp,
            width,
            height,
            buffer,
            pixel_data,
        };
        if let Ok(mut lock) = self.frame.lock() {
            *lock = Some(new_frame);
            self.dirty.store(true, Ordering::Release);
        }
    }

    pub fn get_frame_if_dirty(&self) -> Option<FrameState> {
        if !self.dirty.load(Ordering::Relaxed) {
            return None;
        }
        let lock = self.frame.lock().ok()?;
        self.dirty.store(false, Ordering::Relaxed);
        lock.clone()
    }

    pub fn set_actions(&self, actions: Vec<crate::protocol::VisualAction>) {
        if let Ok(mut lock) = self.actions.lock() {
            *lock = actions;
        }
    }
}

pub static INFERENCE_RUNNING: AtomicBool = AtomicBool::new(false);
pub static SKIP_NEXT_INFERENCE: AtomicBool = AtomicBool::new(false);
