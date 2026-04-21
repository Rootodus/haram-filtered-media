use serde::{Deserialize, Serialize};

/// The binary contract for MessagePack synchronization
#[derive(Debug, Serialize, Deserialize)]
pub struct Metadata {
    pub timestamp: u64,
    pub width: u32,
    pub height: u32,
}

/// The internal representation used by MLProcessor and Renderer
pub struct ContentBuffer<'a> {
    pub meta: Metadata,
    pub pixel_data: &'a [u8],
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VisualAction {
    pub action_type: u8,
    pub rect: [f32; 4],
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProcessedBuffer {
    pub instructions: Vec<VisualAction>,
}
