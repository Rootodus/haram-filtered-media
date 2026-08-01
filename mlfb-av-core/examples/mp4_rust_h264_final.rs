use image::RgbImage;
use mp4::{Mp4Reader, TrackType};
use rust_h264::decoder::Decoder as H264Decoder;
use rust_h264::nal::{NalUnit, NalUnitType, parse_annex_b, parse_avcc};
use std::fs::File;
use std::io::{BufReader, Read, Seek};
use std::sync::Once;

static SAVE_FRAME: Once = Once::new();

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file_path = "assets/video.mp4";
    let file = File::open(file_path)?;
    let file_len = file.metadata()?.len();
    let reader = BufReader::new(file);

    let mut mp4 = Mp4Reader::read_header(reader, file_len)?;

    // Find the video track
    let (track_id, track) = mp4
        .tracks()
        .iter()
        .find_map(|(id, t)| match t.track_type() {
            Ok(TrackType::Video) => Some((*id, t)),
            _ => None,
        })
        .ok_or("No video track found")?;

    println!("Using video track ID: {}", track_id);
    println!("Track box type: {:?}", track.box_type()?);
    println!("Sample count: {}", mp4.sample_count(track_id)?);

    let mut decoder = H264Decoder::new();
    let mut found_sps = false;
    let mut found_pps = false;
    let sample_count = mp4.sample_count(track_id)?;

    // We'll scan samples until we find SPS and PPS.
    for sample_id in 1..=sample_count {
        if let Some(sample) = mp4.read_sample(track_id, sample_id)? {
            let data = &sample.bytes;
            // Try both AVCC and Annex B parsing
            let mut nals = Vec::new();
            // First try AVCC (length-prefixed)
            let avcc_nals = parse_avcc(data, 4);
            if !avcc_nals.is_empty() {
                nals = avcc_nals;
            } else {
                // Try Annex B (start codes)
                nals = parse_annex_b(data)
                    .into_iter()
                    .map(|nal| NalUnit {
                        nal_ref_idc: nal.nal_ref_idc,
                        nal_unit_type: nal.nal_unit_type,
                        rbsp: std::borrow::Cow::Owned(nal.rbsp.to_vec()),
                    })
                    .collect();
            }

            for nal in &nals {
                if nal.nal_unit_type == NalUnitType::Sps && !found_sps {
                    println!("Found SPS at sample {}", sample_id);
                    decoder.decode_nal(nal)?;
                    found_sps = true;
                } else if nal.nal_unit_type == NalUnitType::Pps && !found_pps {
                    println!("Found PPS at sample {}", sample_id);
                    decoder.decode_nal(nal)?;
                    found_pps = true;
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
        eprintln!("SPS/PPS not found in samples");
        return Ok(());
    }

    println!("SPS/PPS found, decoding frames...");

    // Now decode frames, skipping SPS/PPS if they appear again.
    for sample_id in 1..=sample_count {
        if let Some(sample) = mp4.read_sample(track_id, sample_id)? {
            let data = &sample.bytes;
            let mut nals = parse_avcc(data, 4);
            if nals.is_empty() {
                nals = parse_annex_b(data)
                    .into_iter()
                    .map(|nal| NalUnit {
                        nal_ref_idc: nal.nal_ref_idc,
                        nal_unit_type: nal.nal_unit_type,
                        rbsp: std::borrow::Cow::Owned(nal.rbsp.to_vec()),
                    })
                    .collect();
            }
            for nal in &nals {
                // Skip SPS/PPS
                if nal.nal_unit_type == NalUnitType::Sps || nal.nal_unit_type == NalUnitType::Pps {
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
                            if let Err(e) = img.save("mp4_final_frame.png") {
                                eprintln!("Failed to save: {}", e);
                            } else {
                                println!("Saved mp4_final_frame.png");
                            }
                        } else {
                            eprintln!("Failed to create image");
                        }
                    });

                    if SAVE_FRAME.is_completed() {
                        break;
                    }
                }
            }
        }
        if SAVE_FRAME.is_completed() {
            break;
        }
    }

    Ok(())
}
