//! Bridge between UI and main thread: sends commands via a channel.

use crossbeam_channel::Sender;
use hfm_core::pipeline::SeekDelta;
use std::path::PathBuf;

/// Commands that the UI can send to the main thread.
#[derive(Debug, Clone)]
pub enum GuiCommand {
    LoadVideo(PathBuf),
    ToggleVideoFilter,
    ToggleAudioProcessing,
    ChangeVideoBackend(super::Backend),
    ChangeAudioBackend(super::Backend),
    TogglePlayPause,
    Seek(SeekDelta),
    VolumeUp(u8),
    VolumeDown(u8),
    ConfirmSetup,
    BackToSetup,
}

/// Bridge to send GUI commands to the main thread.
#[derive(Clone)]
pub struct Bridge {
    sender: Sender<GuiCommand>,
}

impl Bridge {
    pub fn new(sender: Sender<GuiCommand>) -> Self {
        Self { sender }
    }

    /// Open a file dialog for video selection (spawns a thread).
    pub fn open_video_file(&self) {
        let sender = self.sender.clone();
        std::thread::spawn(move || {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Video files", &["mp4", "mkv", "avi", "mov"])
                .pick_file()
            {
                let _ = sender.send(GuiCommand::LoadVideo(path));
            }
        });
    }

    /// Send a command synchronously (non‑blocking).
    pub fn send(&self, cmd: GuiCommand) {
        let _ = self.sender.send(cmd);
    }
}
