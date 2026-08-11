/// Unified contract for filtering or transforming video frames in-place.
pub trait VideoFilter: Send + Sync {
    fn filter_frame(&self, rgba: &mut [u8], width: u32, height: u32) -> anyhow::Result<()>;
}

/// Unified contract for filtering or transforming real-time audio streams in-place.
pub trait AudioFilter: Send + Sync {
    fn filter_audio(&self, samples: &mut [f32]) -> anyhow::Result<()>;
}
