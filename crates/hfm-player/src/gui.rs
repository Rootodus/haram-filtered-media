//! GUI overlay module.
//! Contains the UI state, bridge for commands, and the egui layout.

mod bridge;
mod state;
mod ui;

pub use bridge::{Bridge, GuiCommand};
pub use state::{AppMode, AppState, Backend, PlaybackState};
pub use ui::ui;
