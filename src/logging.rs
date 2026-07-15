use std::sync::atomic::{AtomicU32, Ordering};

static FRAME_COUNTER: AtomicU32 = AtomicU32::new(0);

/// Increments the global frame counter. Call this once per frame.
pub fn increment_frame_counter() {
    FRAME_COUNTER.fetch_add(1, Ordering::Relaxed);
}

/// Returns `true` if the current frame count is divisible by `interval`.
pub fn should_log(interval: u32) -> bool {
    let count = FRAME_COUNTER.load(Ordering::Relaxed);
    count % interval == 0
}
