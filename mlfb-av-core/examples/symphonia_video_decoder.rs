
use symphonia::default::{get_probe, get_codecs};
use symphonia::core::formats::FormatOptions;
use symphonia::core::formats::probe::Hint;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::codecs::video::VideoDecoder;
use symphonia::core::video::GenericVideoBufferRef;
use image::RgbImage;
use std::sync::Once;
use std::fs::File;

static SAVE_FRAME: Once = Once::new();

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file_path = "assets/video.mp4";
    let source = File::open(file_path)?;
    let mss = MediaSourceStream::new(Box::new(source), Default::default());
    let hint = Hint::new();
    let format_opts = FormatOptions::default();
    let meta_opts = MetadataOptions::default();

    let mut format_reader = get_probe()
        .probe(&hint, mss, format_opts, meta_opts)
        .expect("Failed to probe");

    // Find video track
    let video_track = format_reader
        .tracks()
        .iter()
        .find(|track| {
            track
                .codec_params
                .as_ref()
                .map_or(false, |cp| cp.is_video())
        })
        .cloned()
        .expect("No video track");

    // Get video codec parameters
    let video_params = video_track
        .codec_params
        .as_ref()
        .and_then(|cp| cp.video())
        .expect("Video parameters missing");

    // Get the codec registry and instantiate the video decoder
    let registry = get_codecs();
    let decoder_opts = symphonia::core::codecs::video::VideoDecoderOptions::default();
    let mut decoder = registry
        .make_video_decoder(video_params, &decoder_opts)?;

    while let Ok(Some(packet)) = format_reader.next_packet() {
        if packet.track_id != video_track.id {
            continue;
        }
        // Decode the packet
        if let Ok(buffer) = decoder.decode(&packet) {
            match buffer {
                GenericVideoBufferRef::Rgb(rgb) => {
                    let w = rgb.width() as usize;
                    let h = rgb.height() as usize;
                    let data = rgb.data(); // RGB24 interleaved
                    SAVE_FRAME.call_once(|| {
                        if let Some(img) = RgbImage::from_raw(w as u32, h as u32, data.to_vec()) {
                            if let Err(e) = img.save("symphonia_video_rgb.png") {
                                eprintln!("Failed to save: {}", e);
                            } else {
                                println!("Saved symphonia_video_rgb.png");
                            }
                        } else {
                            eprintln!("Failed to create image");
                        }
                    });
                    break;
                }
                GenericVideoBufferRef::Yuv(yuv) => {
                    // If we get YUV, we can convert manually (BT.709)
                    let w = yuv.width() as usize;
                    let h = yuv.height() as usize;
                    let y_plane = yuv.plane_y(); // &[u8]
                    let u_plane = yuv.plane_u(); // &[u8]
                    let v_plane = yuv.plane_v(); // &[u8]
                    let mut rgb = Vec::with_capacity(w * h * 3);
                    for row in 0..h {
                        for col in 0..w {
                            let y_idx = row * w + col;
                            let uv_idx = (row / 2) * (w / 2) + (col / 2);
                            let y_val = y_plane[y_idx] as f32;
                            let u_val = u_plane[uv_idx] as f32 - 128.0;
                            let v_val = v_plane[uv_idx] as f32 - 128.0;
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
                            if let Err(e) = img.save("symphonia_video_yuv.png") {
                                eprintln!("Failed to save: {}", e);
                            } else {
                                println!("Saved symphonia_video_yuv.png");
                            }
                        } else {
                            eprintln!("Failed to create image");
                        }
                    });
                    break;
                }
                _ => { /* ignore other buffer types */ }
            }
        }
        if SAVE_FRAME.is_completed() {
            break;
        }
    }

    Ok(())
}