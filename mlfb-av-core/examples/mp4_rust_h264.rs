use image::RgbImage;
use mp4::read_mp4;
use rust_h264::decoder::Decoder as H264Decoder;
use rust_h264::nal::{parse_avcc, parse_avcc_config};
use std::sync::Once;

static SAVE_FRAME: Once = Once::new();

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file_path = "assets/video.mp4";
    let file = std::fs::File::open(file_path)?;
    let mp4 = read_mp4(file)?;

    // Find the video track with avcC
    let (track_id, avcc_data) = mp4
        .tracks()
        .iter()
        .find_map(|(id, track)| {
            let stsd = track.stsd.as_ref()?;
            let avc1 = stsd.avc1()?;
            let avc_c = avc1.avc_c.as_ref()?;
            Some((*id, avc_c.data()))
        })
        .expect("No H.264 track with avcC found");

    println!(
        "Found video track {}, avcC length: {}",
        track_id,
        avcc_data.len()
    );

    // Parse SPS/PPS from avcC
    let avcc_config = parse_avcc_config(avcc_data)?;
    let mut decoder = H264Decoder::new();

    // Feed SPS/PPS once
    for nal in avcc_config.sps_nals.iter().chain(&avcc_config.pps_nals) {
        if let Err(e) = decoder.decode_nal(nal) {
            eprintln!("Failed to decode parameter set: {}", e);
        }
    }

    // Get samples for this track
    let samples = mp4.samples(track_id)?;
    let mut frame_count = 0;

    // For the first sample that produces a frame, save it
    for sample in samples {
        // sample.data is the NAL units with length prefixes (AVCC format)
        let nals = parse_avcc(&sample.data, avcc_config.length_size);
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

                        // BT.709 matrix
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
                        if let Err(e) = img.save("mp4_rust_h264_frame.png") {
                            eprintln!("Failed to save: {}", e);
                        } else {
                            println!("Saved mp4_rust_h264_frame.png");
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
