//! Shared type definitions.

pub const ACK_BYTE: u8 = 0x01;
pub const MAX_ACTIONS: usize = 256;
pub const SEQ_LEN: usize = 64;

/// A DOM node extracted from the browser.
#[derive(Debug, Clone)]
pub struct DomNode {
    pub id: u32,
    pub tag: String,
    pub has_text: bool,
    pub text: Option<String>,
    pub rect: Rect,
}

/// A rectangle representing x, y, width, height.
#[derive(Debug, Clone)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone)]
pub struct VisualAction {
    pub action_type: u8,
    pub rect: [f32; 4],
}

/// Data needed for inference: text, nodes, and viewport.
#[derive(Debug, Clone)]
pub struct FrameData {
    pub text: String,
    pub nodes: Vec<DomNode>,
    pub width: u32,
    pub height: u32,
}
