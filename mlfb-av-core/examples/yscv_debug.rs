use image::{ImageBuffer, Rgb};
use std::path::Path;
use yscv_video::Mp4VideoReader;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = "assets/video.mp4";
    let mut reader = Mp4VideoReader::open(Path::new(path))?;

    println!("Codec: {:?}", reader.codec());
    if let Some(audio) = reader.audio_info() {
        println!("Audio: {:?}", audio);
    }
    println!("Total NAL count: {}", reader.nal_count());

    if let Some(frame) = reader.next_frame()? {
        println!("=== Frame 0 ===");
        println!("width: {}", frame.width);
        println!("height: {}", frame.height);
        println!("timestamp_us: {}", frame.timestamp_us);
        println!("keyframe: {}", frame.keyframe);
        println!("bit_depth: {}", frame.bit_depth);
        println!("rgb8_data length: {}", frame.rgb8_data.len());
        println!(
            "rgb16_data: {}",
            if frame.rgb16_data.is_some() {
                "Some"
            } else {
                "None"
            }
        );

        // Save 8-bit RGB
        if !frame.rgb8_data.is_empty() {
            let w = frame.width as u32;
            let h = frame.height as u32;
            if let Some(img) = ImageBuffer::<Rgb<u8>, _>::from_raw(w, h, frame.rgb8_data.clone()) {
                if let Err(e) = img.save("yscv_rgb8.png") {
                    eprintln!("Failed to save rgb8: {}", e);
                } else {
                    println!("Saved yscv_rgb8.png");
                }
            } else {
                eprintln!("Failed to create RgbImage from rgb8 data");
            }
        }

        // Save 16-bit RGB (if present)
        if let Some(rgb16) = &frame.rgb16_data {
            let w = frame.width as u32;
            let h = frame.height as u32;
            // Use ImageBuffer<Rgb<u16>, Vec<u16>>
            if let Some(img) = ImageBuffer::<Rgb<u16>, _>::from_raw(w, h, rgb16.clone()) {
                if let Err(e) = img.save("yscv_rgb16.png") {
                    eprintln!("Failed to save rgb16: {}", e);
                } else {
                    println!("Saved yscv_rgb16.png");
                }
            } else {
                eprintln!("Failed to create Rgb16Image from rgb16 data");
            }
        }

        // Print first few bytes for inspection
        if frame.rgb8_data.len() >= 16 {
            println!("rgb8 first 16 bytes: {:02x?}", &frame.rgb8_data[..16]);
        }
        if let Some(rgb16) = &frame.rgb16_data {
            if rgb16.len() >= 16 {
                println!("rgb16 first 16 values: {:?}", &rgb16[..16]);
            }
        }

        // Try luma-only mode
        let mut reader_luma = Mp4VideoReader::open(Path::new(path))?;
        if let Some(luma_frame) = reader_luma.next_frame_luma_only()? {
            println!("=== Luma-only frame ===");
            println!("width: {}, height: {}", luma_frame.width, luma_frame.height);
            println!("rgb8_data length: {}", luma_frame.rgb8_data.len());
            if luma_frame.rgb8_data.len() >= 16 {
                println!("luma first 16 bytes: {:02x?}", &luma_frame.rgb8_data[..16]);
            }
        } else {
            println!("next_frame_luma_only returned None");
        }
    } else {
        println!("No frames decoded");
    }

    Ok(())
}
