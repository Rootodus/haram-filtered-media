use yscv_video::Mp4VideoReader;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file_path = "assets/video.mp4";
    let mut reader = Mp4VideoReader::open(file_path.as_ref())?;

    let mut frame_count = 0;
    while let Some(frame) = reader.next_frame()? {
        frame_count += 1;
        println!(
            "[TEST] Decoded frame #{}: {}x{} (RGB data len: {})",
            frame_count,
            frame.width,
            frame.height,
            frame.rgb8_data.len()
        );
        if frame_count >= 100 {
            break;
        }
    }
    println!("Done. Total frames decoded: {}", frame_count);
    Ok(())
}
