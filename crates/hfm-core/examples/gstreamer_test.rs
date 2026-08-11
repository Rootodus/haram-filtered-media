use gst::glib::ControlFlow;
use gst::prelude::*;
use gst_app::AppSink;
use gstreamer as gst;
use gstreamer_app as gst_app;
use image::RgbImage;
use std::sync::Once;

static SAVE_FRAME: Once = Once::new();

/// Convert NV12 (Y + interleaved UV) to RGB8 (BT.709).
/// `y` is `width * height` bytes, `uv` is `(width/2) * (height/2) * 2` bytes.
pub fn nv12_to_rgb8(y: &[u8], uv: &[u8], width: usize, height: usize) -> Vec<u8> {
    let mut rgb = Vec::with_capacity(width * height * 3);
    for row in 0..height {
        for col in 0..width {
            let y_idx = row * width + col;
            let uv_idx = (row / 2) * (width / 2) + (col / 2);
            let y_val = y[y_idx] as f32;
            let u_val = uv[uv_idx * 2] as f32 - 128.0;
            let v_val = uv[uv_idx * 2 + 1] as f32 - 128.0;

            // BT.709 matrix
            let r = (y_val + 1.5748 * v_val).clamp(0.0, 255.0) as u8;
            let g = (y_val - 0.1873 * u_val - 0.4681 * v_val).clamp(0.0, 255.0) as u8;
            let b = (y_val + 1.8556 * u_val).clamp(0.0, 255.0) as u8;
            rgb.push(r);
            rgb.push(g);
            rgb.push(b);
        }
    }
    rgb
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize GStreamer
    gst::init()?;

    // Build pipeline
    let pipeline = gst::Pipeline::new();

    // Create elements
    let src = gst::ElementFactory::make("filesrc")
        .property("location", "assets/video.mp4")
        .build()?;

    let decodebin = gst::ElementFactory::make("decodebin").build()?;

    let convert = gst::ElementFactory::make("videoconvert").build()?;

    // Create AppSink without caps – let negotiation decide
    let sink = AppSink::builder().async_(true).drop(true).build();

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

    // Bus watch
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
        let format = structure.get::<&str>("format").unwrap_or("unknown");
        let timestamp = buffer.pts().unwrap_or_default();

        println!("Frame format: {}", format);

        // Map buffer to get pixel data
        let map = buffer.map_readable()?;

        if frame_count == 0 {
            println!(
                "First frame: {}x{}, format: {}, timestamp: {:?}",
                width, height, format, timestamp
            );

            // Handle different pixel formats
            let rgb_data = match format {
                "RGBA" => {
                    // Drop alpha channel
                    let data = map.as_slice();
                    let mut rgb = Vec::with_capacity(width * height * 3);
                    for chunk in data.chunks_exact(4) {
                        rgb.push(chunk[0]);
                        rgb.push(chunk[1]);
                        rgb.push(chunk[2]);
                    }
                    rgb
                }
                "RGB" => map.as_slice().to_vec(),
                "NV12" => {
                    // NV12: Y plane is width*height, UV plane is interleaved
                    let y_plane = &map.as_slice()[0..width * height];
                    let uv_plane = &map.as_slice()[width * height..];
                    nv12_to_rgb8(y_plane, uv_plane, width, height)
                }
                "I420" => {
                    // I420: Y plane, then U, then V (each 1/4 of the image)
                    let y_plane = &map.as_slice()[0..width * height];
                    let u_plane = &map.as_slice()
                        [width * height..width * height + (width / 2) * (height / 2)];
                    let v_plane = &map.as_slice()[width * height + (width / 2) * (height / 2)..];
                    let mut rgb = Vec::with_capacity(width * height * 3);
                    for row in 0..height {
                        for col in 0..width {
                            let y_idx = row * width + col;
                            let uv_idx = (row / 2) * (width / 2) + (col / 2);
                            let y_val = y_plane[y_idx] as f32;
                            let u_val = u_plane[uv_idx] as f32 - 128.0;
                            let v_val = v_plane[uv_idx] as f32 - 128.0;
                            // BT.709
                            let r = (y_val + 1.5748 * v_val).clamp(0.0, 255.0) as u8;
                            let g =
                                (y_val - 0.1873 * u_val - 0.4681 * v_val).clamp(0.0, 255.0) as u8;
                            let b = (y_val + 1.8556 * u_val).clamp(0.0, 255.0) as u8;
                            rgb.push(r);
                            rgb.push(g);
                            rgb.push(b);
                        }
                    }
                    rgb
                }
                _ => {
                    eprintln!("Unsupported format: {}", format);
                    return Ok(());
                }
            };

            SAVE_FRAME.call_once(|| {
                if let Some(img) = RgbImage::from_raw(width as u32, height as u32, rgb_data) {
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

        frame_count += 1;
        if frame_count % 30 == 0 {
            let elapsed = start.elapsed();
            let fps = frame_count as f64 / elapsed.as_secs_f64();
            println!("Frames: {}, FPS: {:.1}", frame_count, fps);
        }

        if frame_count >= 300 {
            break;
        }
    }

    // Clean up
    pipeline.set_state(gst::State::Null)?;

    Ok(())
}
