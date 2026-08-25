//! Configuration constants for the player.

pub const SAMPLE_RATE: u32 = 44100;
pub const CHANNELS: u16 = 2;
pub const SEEK_DELTA_NS: i64 = 10_000_000_000; // 10 seconds
pub const WINDOW_SAMPLES: usize = 343_980;
pub const SEEK_DEBOUNCE_MS: u64 = 200;
pub const SYNC_TOLERANCE_NS: i64 = 50_000_000; // 50 ms
