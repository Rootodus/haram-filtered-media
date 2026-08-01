use image::RgbImage;
use std::sync::Once;
use yscv_video::Mp4VideoReader;

static SAVE_YUV_FRAME: Once = Once::new();

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file_path = "assets/video.mp4";
    let mut reader = Mp4VideoReader::open(file_path.as_ref())?;

    // Read the first frame (luma only)
    while let Some(frame) = reader.next_frame_luma_only()? {
        let y = &frame.y;
        let u = &frame.u;
        let v = &frame.v;
        let width = frame.width as usize;
        let height = frame.height as usize;

        println!("Y first 10: {:?}", &y[..10]);
        println!("U first 10: {:?}", &u[..10]);
        println!("V first 10: {:?}", &v[..10]);

        // Convert YUV to RGB (BT.709, limited range)
        let mut rgb = Vec::with_capacity(width * height * 3);
        for row in 0..height {
            for col in 0..width {
                let y_idx = row * width + col;
                let uv_idx = (row / 2) * (width / 2) + (col / 2);
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

        SAVE_YUV_FRAME.call_once(|| {
            if let Some(img) = RgbImage::from_raw(width as u32, height as u32, rgb.clone()) {
                if let Err(e) = img.save("yuv_frame.png") {
                    eprintln!("Failed to save: {}", e);
                } else {
                    println!("Saved yuv_frame.png");
                }
            } else {
                eprintln!("Failed to create RGB image");
            }
        });

        // Only first frame
        break;
    }

    Ok(())
}
