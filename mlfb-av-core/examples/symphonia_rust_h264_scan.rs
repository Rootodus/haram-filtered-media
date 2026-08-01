use image::RgbImage;
use rust_h264::decoder::Decoder as H264Decoder;
use rust_h264::nal::{NalUnit, NalUnitType, parse_annex_b, parse_avcc};
use std::fs::File;
use std::sync::Once;
use symphonia::core::formats::FormatOptions;
use symphonia::core::formats::probe::Hint;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::default::get_probe;

static SAVE_FRAME: Once = Once::new();

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file_path = "assets/video.mp4";
    let source = File::open(file_path)?;
    let mss = MediaSourceStream::new(Box::new(source), Default::default());

    let hint = Hint::new();
    let format_opts = FormatOptions::default();
    let metadata_opts = MetadataOptions::default();

    let mut format_reader = get_probe()
        .probe(&hint, mss, format_opts, metadata_opts)
        .expect("Failed to probe file");

    // Find video track
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

    let mut decoder = H264Decoder::new();
    let mut found_sps = false;
    let mut found_pps = false;

    // We'll scan packets until we find SPS/PPS.
    let mut packet_count = 0;
    while let Ok(Some(packet)) = format_reader.next_packet() {
        if packet.track_id != video_track.id {
            continue;
        }
        packet_count += 1;
        let data = &packet.data;
        // Try AVCC first (length-prefixed, length size 4)
        let mut nals = parse_avcc(data, 4);
        if nals.is_empty() {
            // Try Annex B
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
                println!("Found SPS in packet {}", packet_count);
                decoder.decode_nal(nal)?;
                found_sps = true;
            } else if nal.nal_unit_type == NalUnitType::Pps && !found_pps {
                println!("Found PPS in packet {}", packet_count);
                decoder.decode_nal(nal)?;
                found_pps = true;
            }
            if found_sps && found_pps {
                break;
            }
        }
        if found_sps && found_pps {
            break;
        }
        // Limit scanning to first 100 packets to avoid infinite loop
        if packet_count > 100 {
            break;
        }
    }

    if !found_sps || !found_pps {
        eprintln!("SPS/PPS not found in first 100 packets");
        return Ok(());
    }

    println!("SPS/PPS found, decoding frames...");

    // Now decode frames
    packet_count = 0;
    let mut frame_count = 0;
    while let Ok(Some(packet)) = format_reader.next_packet() {
        if packet.track_id != video_track.id {
            continue;
        }
        packet_count += 1;
        let data = &packet.data;
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
            if nal.nal_unit_type == NalUnitType::Sps || nal.nal_unit_type == NalUnitType::Pps {
                continue;
            }
            if let Ok(Some(frame)) = decoder.decode_nal(nal) {
                frame_count += 1;
                println!(
                    "Decoded frame {}: {}x{}",
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

                if SAVE_FRAME.is_completed() {
                    break;
                }
            }
        }
        if SAVE_FRAME.is_completed() {
            break;
        }
        if packet_count > 500 {
            break;
        }
    }

    Ok(())
}
