//! Shared type definitions.

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
