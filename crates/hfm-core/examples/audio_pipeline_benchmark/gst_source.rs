use gst::glib::ControlFlow;
use gst::{ClockTime, SeekFlags};
use gstreamer as gst;
use gstreamer::glib::object::Cast;
use gstreamer::prelude::GstBinExtManual;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use hfm_core::pipeline::{FrameSource, HEIGHT, WIDTH};

pub struct GstSource {
    pipeline: gst::Pipeline,
    video_sink: gst_app::AppSink,
    audio_sink: gst_app::AppSink,
    _seekable: bool,
}

impl GstSource {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        gst::init()?;

        let pipeline = gst::Pipeline::new();
        let video_path = format!("{}/assets/video.mp4", env!("CARGO_MANIFEST_DIR"));
        let src = gst::ElementFactory::make("filesrc")
            .property("location", video_path)
            .build()?;
        let decodebin = gst::ElementFactory::make("decodebin").build()?;

        // ---- Video branch ----
        let video_convert = gst::ElementFactory::make("videoconvert").build()?;
        let video_scale = gst::ElementFactory::make("videoscale").build()?;
        let video_sink = gst_app::AppSink::builder()
            .caps(
                &gst::Caps::builder("video/x-raw")
                    .field("format", "RGBA")
                    .field("width", WIDTH as i32)
                    .field("height", HEIGHT as i32)
                    .build(),
            )
            .async_(true)
            .drop(true)
            .build();
        let video_sink_element = video_sink.upcast_ref::<gst::Element>().clone();

        // ---- Audio branch ----
        let audio_convert = gst::ElementFactory::make("audioconvert").build()?;
        let audio_resample = gst::ElementFactory::make("audioresample").build()?;
        // Model expects 44.1 kHz, stereo, f32.
        let audio_sink = gst_app::AppSink::builder()
            .caps(
                &gst::Caps::builder("audio/x-raw")
                    .field("format", "F32LE")
                    .field("rate", 44100 as i32)
                    .field("channels", 2 as i32)
                    .field("layout", "interleaved")
                    .build(),
            )
            .async_(true)
            .drop(true)
            .build();
        let audio_sink_element = audio_sink.upcast_ref::<gst::Element>().clone();

        // Add all elements to the pipeline
        pipeline.add_many(&[
            &src,
            &decodebin,
            &video_convert,
            &video_scale,
            &video_sink_element,
            &audio_convert,
            &audio_resample,
            &audio_sink_element,
        ])?;

        // Link static elements: src -> decodebin (decoding will be dynamic)
        gst::Element::link_many(&[&src, &decodebin])?;

        // We will link the video and audio branches via pad-added signals.

        let video_convert_clone = video_convert.clone();
        let audio_convert_clone = audio_convert.clone();
        let video_scale_clone = video_scale.clone();
        let audio_resample_clone = audio_resample.clone();
        let video_sink_element_clone = video_sink_element.clone();
        let audio_sink_element_clone = audio_sink_element.clone();

        decodebin.connect_pad_added(move |_, src_pad| {
            let caps = src_pad.current_caps().expect("Failed to get caps");
            let structure = caps.structure(0).expect("No structure");
            let name = structure.name();

            if name.starts_with("video/") {
                // Link video pad to video branch
                let sink_pad = video_convert_clone
                    .static_pad("sink")
                    .expect("videoconvert has no sink pad");
                if !sink_pad.is_linked() {
                    let src_pad = src_pad.clone();
                    if let Err(e) = src_pad.link(&sink_pad) {
                        eprintln!("Failed to link video pad to videoconvert: {}", e);
                    } else {
                        // Link the rest of the video chain: convert -> scale -> sink
                        if let Err(e) = gst::Element::link_many(&[
                            &video_convert_clone,
                            &video_scale_clone,
                            &video_sink_element_clone,
                        ]) {
                            eprintln!("Failed to link video branch: {}", e);
                        }
                    }
                }
            } else if name.starts_with("audio/") {
                // Link audio pad to audio branch
                let sink_pad = audio_convert_clone
                    .static_pad("sink")
                    .expect("audioconvert has no sink pad");
                if !sink_pad.is_linked() {
                    let src_pad = src_pad.clone();
                    if let Err(e) = src_pad.link(&sink_pad) {
                        eprintln!("Failed to link audio pad to audioconvert: {}", e);
                    } else {
                        // Link the rest of the audio chain: convert -> resample -> sink
                        if let Err(e) = gst::Element::link_many(&[
                            &audio_convert_clone,
                            &audio_resample_clone,
                            &audio_sink_element_clone,
                        ]) {
                            eprintln!("Failed to link audio branch: {}", e);
                        }
                    }
                }
            }
        });

        pipeline.set_state(gst::State::Playing)?;

        let bus = pipeline.bus().expect("No bus");
        let _guard = bus.add_watch(move |_, msg| {
            use gst::MessageView;
            match msg.view() {
                MessageView::Error(err) => {
                    eprintln!("GStreamer error: {}", err.error());
                    if let Some(debug) = err.debug() {
                        eprintln!("Debug info: {}", debug);
                    }
                    ControlFlow::Break
                }
                MessageView::Eos(_) => {
                    println!("End of stream");
                    ControlFlow::Break
                }
                _ => ControlFlow::Continue,
            }
        })?;

        Ok(Self {
            pipeline,
            video_sink,
            audio_sink,
            _seekable: true,
        })
    }

    // Pull a video frame (existing)
    pub fn pull_video_frame(&mut self) -> Option<(Vec<u8>, u64)> {
        loop {
            match self.video_sink.pull_sample() {
                Ok(sample) => {
                    let buffer = sample.buffer()?;
                    let map = buffer.map_readable().ok()?;
                    let data = map.as_slice().to_vec();
                    let pts_ns = buffer.pts().map(|c| c.nseconds()).unwrap_or(0);
                    return Some((data, pts_ns));
                }
                Err(_) => {
                    if self.video_sink.is_eos() {
                        return None;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
            }
        }
    }

    // Pull an audio frame (new)
    pub fn pull_audio_frame(&mut self) -> Option<(Vec<f32>, u64)> {
        loop {
            match self.audio_sink.pull_sample() {
                Ok(sample) => {
                    let buffer = sample.buffer()?;
                    let map = buffer.map_readable().ok()?;
                    // The audio data is F32LE interleaved.
                    let data = map.as_slice();
                    // We need to reinterpret as f32.
                    let samples = unsafe {
                        std::slice::from_raw_parts(data.as_ptr() as *const f32, data.len() / 4)
                    };
                    let pts_ns = buffer.pts().map(|c| c.nseconds()).unwrap_or(0);
                    return Some((samples.to_vec(), pts_ns));
                }
                Err(_) => {
                    if self.audio_sink.is_eos() {
                        return None;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
            }
        }
    }
}

impl FrameSource for GstSource {
    // `pull_frame` for the video source (kept for compatibility with existing pipeline)
    fn pull_frame(&mut self) -> Option<(Vec<u8>, u64)> {
        self.pull_video_frame()
    }

    fn seek(&mut self, delta_ns: i64) -> Result<(), String> {
        let current_pos = self
            .pipeline
            .query_position::<ClockTime>()
            .unwrap_or_else(|| ClockTime::from_seconds(0));
        let current_ns = current_pos.nseconds() as i64;
        let new_ns = (current_ns + delta_ns).max(0);
        let new_pos = ClockTime::from_nseconds(new_ns as u64);
        self.pipeline
            .seek_simple(SeekFlags::FLUSH, new_pos)
            .map_err(|e| format!("Seek failed: {:?}", e))
    }
}
