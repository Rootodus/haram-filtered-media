//! Pure audio processing worker.
//!
//! This module transforms raw audio chunks into processed speech PCM using
//! the HT-Demucs ONNX model.
//!
//! It does not know about:
//! - GStreamer
//! - CPAL playback
//! - wgpu rendering
//! - buffering state
//! - video
//!
//! It only:
//! 1. Receives `RawAudioChunk`
//! 2. Runs the model
//! 3. Emits `ProcessedAudioChunk` with the same PTS and generation
//!
//! NOTE: This is an interface-first skeleton. Real inference logic is
//! intentionally absent.

use std::sync::Arc;
use std::thread::JoinHandle;

use crossbeam_channel::{Receiver, Sender};

use crate::sync::SeekGeneration;
use crate::types::{ProcessedAudioChunk, RawAudioChunk};

/// Spawn the audio processor worker thread.
///
/// The returned handle belongs to the caller and should be kept alive.
pub fn spawn_audio_processor(
    rx: Receiver<RawAudioChunk>,
    tx: Sender<ProcessedAudioChunk>,
    _generation: Arc<SeekGeneration>,
) -> JoinHandle<()> {
    std::thread::spawn(move || {
        // Stub: the real loop will block on `rx`, run inference, and send
        // processed chunks to `tx`.
        let _ = (rx, tx);
        todo!("audio_processor is not implemented yet")
    })
}
