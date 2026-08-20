//! Renderer stub.
//!
//! This module is intentionally empty for the interface-first design pass.
//! It exists only to let `main.rs` compile and express its intent.
//!
//! Later, the real wgpu renderer from the earlier implementation will be
//! inserted here unchanged.

use std::sync::Arc;
use winit::window::Window;

pub struct Renderer {
    _window: Arc<Window>,
}

impl Renderer {
    pub async fn new(window: Arc<Window>) -> Self {
        Self { _window: window }
    }

    pub fn render(&mut self, _frame_data: Option<Vec<u8>>) {
        // Stub: real wgpu rendering will go here.
    }

    pub fn resize(&mut self, _width: u32, _height: u32) {
        // Stub: real surface resize will go here.
    }
}
