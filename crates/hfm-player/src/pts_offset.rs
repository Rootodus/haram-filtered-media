//! PTS offset management for audio/video sync.

#[derive(Debug, Clone, Copy)]
pub struct SyncState {
    pub offset: Option<i64>,
    pub synced: bool,
}

impl SyncState {
    pub fn new() -> Self {
        Self {
            offset: None,
            synced: false,
        }
    }

    /// Reset the offset so it will be recomputed on the next frame.
    pub fn reset(&mut self) {
        self.offset = None;
        self.synced = false;
    }

    /// Compute the offset based on the first video PTS and the current audio clock.
    /// Returns the adjusted PTS (front_pts - offset) if offset exists, else front_pts as i64.
    pub fn adjust_pts(&mut self, front_pts: u64, audio_now: u64) -> i64 {
        if self.offset.is_none() {
            let offset = front_pts as i64 - audio_now as i64;
            self.offset = Some(offset);
            // Optionally log the initial offset here.
        }
        front_pts as i64 - self.offset.unwrap_or(0)
    }
}
