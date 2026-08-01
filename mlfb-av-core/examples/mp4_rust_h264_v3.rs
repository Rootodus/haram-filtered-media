use image::RgbImage;
use mp4::read_mp4;
use rust_h264::decoder::Decoder as H264Decoder;
use rust_h264::nal::{NalUnit, NalUnitType, parse_avcc};
use std::fs::File;
use std::sync::Once;

static SAVE_FRAME: Once = Once::new();

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file_path = "assets/video.mp4";
    let file = File::open(file_path)?;
    let mut mp4 = read_mp4(file)?;

    // Find the video track
    let (track_id, track) = mp4
        .tracks()
        .iter()
        .find_map(|(id, t)| match t.track_type() {
            Ok(mp4::TrackType::Video) => Some((*id, t)),
            _ => None,
        })
        .ok_or("No video track found")?;

    println!("Using video track ID: {}", track_id);
    let sample_count = mp4.sample_count(track_id)?;
    println!("Total samples: {}", sample_count);

    // Length size for AVCC format – typical is 4 bytes.
    let length_size = 4;

    let mut decoder = H264Decoder::new();
    let mut found_sps = false;
    let mut found_pps = false;

    // Scan samples to find SPS and PPS
    for sample_id in 1..=sample_count {
        if let Some(sample) = mp4.read_sample(track_id, sample_id)? {
            let nals = parse_avcc(&sample.bytes, length_size);
            for nal in &nals {
                if let NalUnitType::Sps = nal.nal_unit_type {
                    if !found_sps {
                        println!("Found SPS, length: {}", nal.rbsp.len());
                        decoder.decode_nal(nal)?;
                        found_sps = true;
                    }
                } else if let NalUnitType::Pps = nal.nal_unit_type {
                    if !found_pps {
                        println!("Found PPS, length: {}", nal.rbsp.len());
                        decoder.decode_nal(nal)?;
                        found_pps = true;
                    }
                }
                if found_sps && found_pps {
                    break;
                }
            }
        }
        if found_sps && found_pps {
            break;
        }
    }

    if !found_sps || !found_pps {
        eprintln!("Could not find SPS and PPS in samples");
        return Ok(());
    }

    println!("SPS/PPS found, starting decode...");

    // Now decode frames
    for sample_id in 1..=sample_count {
        if let Some(sample) = mp4.read_sample(track_id, sample_id)? {
            let nals = parse_avcc(&sample.bytes, length_size);
            for nal in &nals {
                // Skip SPS/PPS if they appear again
                if let NalUnitType::Sps | NalUnitType::Pps = nal.nal_unit_type {
                    continue;
                }
                if let Ok(Some(frame)) = decoder.decode_nal(nal) {
                    println!("Decoded frame: {}x{}", frame.width, frame.height);
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
                            let g =
                                (y_val - 0.1873 * u_val - 0.4681 * v_val).clamp(0.0, 255.0) as u8;
                            let b = (y_val + 1.8556 * u_val).clamp(0.0, 255.0) as u8;
                            rgb.push(r);
                            rgb.push(g);
                            rgb.push(b);
                        }
                    }

                    SAVE_FRAME.call_once(|| {
                        if let Some(img) = RgbImage::from_raw(w as u32, h as u32, rgb) {
                            if let Err(e) = img.save("mp4_frame_v3.png") {
                                eprintln!("Failed to save: {}", e);
                            } else {
                                println!("Saved mp4_frame_v3.png");
                            }
                        } else {
                            eprintln!("Failed to create image");
                        }
                    });

                    break; // only first frame
                }
            }
        }
        if SAVE_FRAME.is_completed() {
            break;
        }
    }

    Ok(())
}
