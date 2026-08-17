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
    sink: gst_app::AppSink,
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
        let convert = gst::ElementFactory::make("videoconvert").build()?;
        let scale = gst::ElementFactory::make("videoscale").build()?;
        let sink = gst_app::AppSink::builder()
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
        let sink_element = sink.upcast_ref::<gst::Element>().clone();

        pipeline.add_many(&[&src, &decodebin, &convert, &scale, &sink_element])?;
        gst::Element::link_many(&[&src, &decodebin])?;
        gst::Element::link_many(&[&convert, &scale, &sink_element])?;

        let convert_clone = convert.clone();
        decodebin.connect_pad_added(move |_, src_pad| {
            let caps = src_pad.current_caps().expect("Failed to get caps");
            let structure = caps.structure(0).expect("No structure");
            if structure.name().starts_with("video/") {
                let sink_pad = convert_clone
                    .static_pad("sink")
                    .expect("convert has no sink pad");
                if sink_pad.is_linked() {
                    return;
                }
                let src_pad = src_pad.clone();
                if let Err(e) = src_pad.link(&sink_pad) {
                    eprintln!("Failed to link decodebin pad to convert: {}", e);
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
            sink,
            _seekable: true,
        })
    }
}

impl FrameSource for GstSource {
    fn pull_frame(&mut self) -> Option<(Vec<u8>, u64)> {
        loop {
            match self.sink.pull_sample() {
                Ok(sample) => {
                    let buffer = sample.buffer()?;
                    let map = buffer.map_readable().ok()?;
                    let data = map.as_slice().to_vec();
                    let pts_ns = buffer.pts().map(|c| c.nseconds()).unwrap_or(0);
                    return Some((data, pts_ns));
                }
                Err(_) => {
                    if self.sink.is_eos() {
                        return None;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
            }
        }
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
