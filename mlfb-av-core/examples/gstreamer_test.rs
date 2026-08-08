use gst::glib::ControlFlow;
use gst::prelude::*;
use gst_app::AppSink;
use gstreamer as gst;
use gstreamer_app as gst_app;
use image::RgbImage;
use std::sync::Once;

static SAVE_FRAME: Once = Once::new();

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize GStreamer
    gst::init()?;

    // Build pipeline
    let pipeline = gst::Pipeline::new();

    // Create elements using ElementFactory::make() + builder pattern
    let src = gst::ElementFactory::make("filesrc")
        .property("location", "assets/video.mp4")
        .build()?;

    let decodebin = gst::ElementFactory::make("decodebin").build()?;

    let convert = gst::ElementFactory::make("videoconvert").build()?;

    // Create AppSink using AppSink::builder()
    let sink = AppSink::builder()
        .caps(
            &gst::Caps::builder("video/x-raw")
                .field("format", "RGBA")
                .field("width", 0) // 0 means "any"
                .field("height", 0) // 0 means "any"
                .build(),
        )
        .async_(true)
        .drop(true)
        .build();

    let sink_element = sink.upcast_ref::<gst::Element>().clone();

    // Add elements to pipeline
    pipeline.add_many(&[&src, &decodebin, &convert, &sink_element])?;
    gst::Element::link_many(&[&src, &decodebin])?;
    gst::Element::link_many(&[&convert, &sink_element])?;

    // Connect pad-added signal on decodebin to link to convert
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

    // Set pipeline to playing
    pipeline.set_state(gst::State::Playing)?;

    // Set up bus watch for errors and EOS
    let bus = pipeline.bus().expect("Failed to get bus");
    let _watch_id = bus.add_watch(move |_, msg| {
        use gst::MessageView;
        match msg.view() {
            MessageView::Error(err) => {
                eprintln!("Error: {}", err.error());
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

    let mut frame_count = 0;
    let start = std::time::Instant::now();

    // Pull samples
    while let Ok(sample) = sink.pull_sample() {
        let buffer = sample.buffer().expect("No buffer");
        let caps = sample.caps().expect("No caps");
        let structure = caps.structure(0).expect("No structure");
        let width = structure.get::<i32>("width").unwrap_or(0) as usize;
        let height = structure.get::<i32>("height").unwrap_or(0) as usize;
        let timestamp = buffer.pts().unwrap_or_default();

        // Map buffer to get pixel data
        let map = buffer.map_readable()?;

        if frame_count == 0 {
            println!(
                "First frame: {}x{}, timestamp: {:?}",
                width, height, timestamp
            );
            // Save as PNG
            if width > 0 && height > 0 && map.len() >= width * height * 4 {
                let data = map.as_slice();
                SAVE_FRAME.call_once(|| {
                    if let Some(img) =
                        RgbImage::from_raw(width as u32, height as u32, data.to_vec())
                    {
                        if let Err(e) = img.save("gst_frame.png") {
                            eprintln!("Failed to save PNG: {}", e);
                        } else {
                            println!("Saved gst_frame.png");
                        }
                    } else {
                        eprintln!("Failed to create image");
                    }
                });
            }
        }

        frame_count += 1;
        if frame_count % 30 == 0 {
            let elapsed = start.elapsed();
            let fps = frame_count as f64 / elapsed.as_secs_f64();
            println!("Frames: {}, FPS: {:.1}", frame_count, fps);
        }

        // Break after 300 frames
        if frame_count >= 300 {
            break;
        }
    }

    // Clean up
    pipeline.set_state(gst::State::Null)?;
    // The watch is dropped automatically when bus is dropped

    Ok(())
}
