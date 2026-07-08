use chromiumoxide::{Page, page::ScreenshotParams};
use image::ImageFormat;
use std::error::Error;

/// Captures a full-page screenshot, decodes PNG to RGBA, and returns (width, height, pixel_data).
pub async fn capture_screenshot(page: &Page) -> Result<(u32, u32, Vec<u8>), Box<dyn Error>> {
    // Capture as PNG bytes with default parameters
    let png_data = page.screenshot(ScreenshotParams::default()).await?;

    // Decode PNG using the `image` crate
    let img = image::load_from_memory_with_format(&png_data, ImageFormat::Png)?;
    let rgb_img = img.to_rgba8();
    let (width, height) = rgb_img.dimensions();
    let pixel_data = rgb_img.into_raw(); // Vec<u8> in RGBA format

    Ok((width, height, pixel_data))
}
