//! Audio output worker.
//!
//! This module owns CPAL playback and consumes `ProcessedAudioChunk`s.
//!
//! It does not know about:
//! - GStreamer
//! - ONNX
//! - HT-Demucs
//! - video
//!
//! It only:
//! 1. Receives processed PCM chunks
//! 2. Sends them to the audio device
//! 3. Advances the shared `AudioClock`
//! 4. Updates the `BufferingFlag` based on buffer occupancy
//!
//! NOTE: This is an interface-first skeleton. Real CPAL logic is
//! intentionally absent.

use std::sync::Arc;
use std::thread::JoinHandle;

use crossbeam_channel::Receiver;

use crate::sync::{AudioClock, BufferingFlag};
use crate::types::ProcessedAudioChunk;

/// Spawn the audio output worker thread.
///
/// `sample_rate` and `channels` describe the output device format.
/// The returned handle must be kept alive by the caller.
pub fn spawn_audio_output(
    rx: Receiver<ProcessedAudioChunk>,
    audio_clock: Arc<AudioClock>,
    buffering: Arc<BufferingFlag>,
    _sample_rate: u32,
    _channels: u16,
) -> JoinHandle<()> {
    std::thread::spawn(move || {
        // Stub: the real loop will consume `rx`, push samples to CPAL,
        // advance `audio_clock`, and update `buffering`.
        let _ = (rx, audio_clock, buffering);
        todo!("audio_output is not implemented yet")
    })
}
