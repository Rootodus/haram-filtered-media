//! Audio constants – no runtime setup, used by examples and later integration.

pub const SAMPLE_RATE: u32 = 48_000;
pub const CHANNELS: u16 = 2;
pub const CALLBACK_FRAMES: usize = 1024; // samples per channel
pub const CALLBACK_SAMPLES: usize = CALLBACK_FRAMES * CHANNELS as usize;
pub const SPSC_CAPACITY: usize = 16384; // enough for ~170ms @ 48kHz
