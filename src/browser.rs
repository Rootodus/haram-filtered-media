//! Browser automation module using chromiumoxide.

pub mod extract;
pub mod screenshot;
pub mod session;

pub use crate::types::DomNode;
pub use extract::extract_dom_nodes;
pub use screenshot::capture_screenshot;
pub use session::BrowserSession;
