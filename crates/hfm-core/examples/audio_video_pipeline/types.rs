//! Shared plain-data message types.
//!
//! No behavior, no GStreamer/ONNX/CPAL/wgpu dependencies.
//! Every audio/video message carries:
//! - `pts_ns`: presentation timestamp in nanoseconds
//! - `generation`: seek generation used to discard stale data

#![allow(dead_code)]

/// Raw audio chunk pulled from GStreamer.
pub struct RawAudioChunk {
    /// Interleaved PCM samples.
    pub samples: Vec<f32>,
    /// PTS of the first sample in this chunk.
    pub pts_ns: u64,
    /// Seek generation at the time this chunk was pulled.
    pub generation: u64,
}

/// Processed audio chunk produced by the HT-Demucs worker.
pub struct ProcessedAudioChunk {
    /// Interleaved processed PCM samples.
    pub samples: Vec<f32>,
    /// PTS of the first output sample.
    pub pts_ns: u64,
    /// Generation used during inference.
    pub generation: u64,
}

/// Raw video frame pulled from GStreamer.
pub struct RawVideoFrame {
    /// RGBA pixel data.
    pub data: Vec<u8>,
    /// Presentation timestamp of the frame.
    pub pts_ns: u64,
    /// Seek generation at pull time.
    pub generation: u64,
}

/// Processed video frame produced by the PPHumanSeg worker.
pub struct ProcessedVideoFrame {
    /// Processed RGBA pixel data.
    pub data: Vec<u8>,
    /// Same PTS as the input frame.
    pub pts_ns: u64,
    /// Generation used during processing.
    pub generation: u64,
}
