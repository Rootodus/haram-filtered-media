//! GStreamer source adapter.
//!
//! This module is responsible only for owning a GStreamer pipeline and
//! exposing raw video/audio buffers to the rest of the application.
//!
//! It must not know about:
//! - ONNX inference
//! - CPAL audio playback
//! - wgpu rendering
//! - playback / buffering state
//!
//! It only produces raw data packages and performs seeks.
//!
//! NOTE: This is an interface-first skeleton. Real GStreamer logic is
//! intentionally absent.

pub struct GstSource {
    // Real pipeline fields will be added here later.
    _private: (),
}

impl GstSource {
    /// Create a new GStreamer source.
    ///
    /// In the real implementation this will:
    /// - initialize GStreamer
    /// - build the decodebin pipeline
    /// - create video and audio AppSinks
    /// - set the pipeline to Playing
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        // Stub: successful construction, no real pipeline yet.
        Ok(Self { _private: () })
    }

    /// Try to pull the next raw video frame.
    ///
    /// Returns `None` when no frame is currently available or on EOS.
    pub fn try_pull_video_frame(&self) -> Option<(Vec<u8>, u64)> {
        todo!("GstSource::try_pull_video_frame")
    }

    /// Try to pull the next raw audio chunk.
    ///
    /// Returns `None` when no chunk is currently available or on EOS.
    pub fn try_pull_audio_frame(&self) -> Option<(Vec<f32>, u64)> {
        todo!("GstSource::try_pull_audio_frame")
    }

    /// Seek the pipeline by `delta_ns`.
    ///
    /// Real implementation will:
    /// - compute the new position
    /// - call `seek_simple`
    /// - return `Ok(())` on success
    pub fn seek(&mut self, delta_ns: i64) -> Result<(), String> {
        let _ = delta_ns;
        todo!("GstSource::seek")
    }
}
