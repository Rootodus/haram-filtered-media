use image::RgbImage;
use rust_h264::decoder::Decoder as H264Decoder;
use rust_h264::nal::{parse_avcc, parse_avcc_config};
use std::sync::Once;
use symphonia::core::formats::FormatOptions;
use symphonia::core::formats::probe::Hint;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::default::get_probe;

static SAVE_FRAME: Once = Once::new();

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file_path = "assets/video.mp4";
    let source = std::fs::File::open(file_path)?;
    let mss = MediaSourceStream::new(Box::new(source), Default::default());

    let hint = Hint::new();
    let format_opts = FormatOptions::default();
    let metadata_opts = MetadataOptions::default();

    let mut format_reader = get_probe()
        .probe(&hint, mss, format_opts, metadata_opts)
        .expect("Failed to probe file");

    // Find the video track
    let video_track = format_reader
        .tracks()
        .iter()
        .find(|track| {
            track
                .codec_params
                .as_ref()
                .and_then(|cp| cp.video())
                .map_or(false, |vp| vp.width.is_some() && vp.height.is_some())
        })
        .cloned()
        .expect("No video track found");

    println!("Using track id: {}", video_track.id);

    // Get the video codec parameters
    let video_params = video_track
        .codec_params
        .as_ref()
        .and_then(|cp| cp.video())
        .expect("Video codec parameters missing");

    // Take the first extra data (should be avcC for H.264)
    let avcc_data = video_params
        .extra_data
        .first()
        .expect("No extra data found")
        .data
        .as_ref();

    println!("Found avcC data of length: {}", avcc_data.len());

    // Parse avcC to get SPS/PPS
    let avcc_config = parse_avcc_config(avcc_data)?;
    let mut decoder = H264Decoder::new();

    // Feed SPS/PPS
    for nal in avcc_config.sps_nals.iter().chain(&avcc_config.pps_nals) {
        decoder.decode_nal(nal)?;
    }

    let mut frame_count = 0;

    // Read packets and decode
    while let Ok(Some(packet)) = format_reader.next_packet() {
        if packet.track_id != video_track.id {
            continue;
        }
        // Parse AVCC format NALs from the packet data
        let nals = parse_avcc(&packet.data, avcc_config.length_size);
        for nal in &nals {
            if let Ok(Some(frame)) = decoder.decode_nal(nal) {
                frame_count += 1;
                println!(
                    "Decoded frame #{}: {}x{}",
                    frame_count, frame.width, frame.height
                );

                // Convert YUV420 to RGB (BT.709)
                let y = &frame.y;
                let u = &frame.u;
                let v = &frame.v;
                let w = frame.width as usize;
                let h = frame.height as usize;
                let mut rgb = Vec::with_capacity(w * h * 3);
                for row in 0..h {
                    for col in 0..w {
                        let y_idx = row * w + col;
                        let uv_idx = (row / 2) * (w / 2) + (col / 2);
                        let y_val = y[y_idx] as f32;
                        let u_val = u[uv_idx] as f32 - 128.0;
                        let v_val = v[uv_idx] as f32 - 128.0;

                        // BT.709 matrix (common for HD)
                        let r = (y_val + 1.5748 * v_val).clamp(0.0, 255.0) as u8;
                        let g = (y_val - 0.1873 * u_val - 0.4681 * v_val).clamp(0.0, 255.0) as u8;
                        let b = (y_val + 1.8556 * u_val).clamp(0.0, 255.0) as u8;
                        rgb.push(r);
                        rgb.push(g);
                        rgb.push(b);
                    }
                }

                SAVE_FRAME.call_once(|| {
                    if let Some(img) = RgbImage::from_raw(w as u32, h as u32, rgb) {
                        if let Err(e) = img.save("symphonia_frame.png") {
                            eprintln!("Failed to save: {}", e);
                        } else {
                            println!("Saved symphonia_frame.png");
                        }
                    } else {
                        eprintln!("Failed to create image");
                    }
                });

                // Only first frame
                break;
            }
        }
        if SAVE_FRAME.is_completed() {
            break;
        }
    }

    Ok(())
}
