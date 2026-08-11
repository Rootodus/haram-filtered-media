use crate::filter::VideoFilter;
use anyhow::{Result, anyhow};

#[derive(Debug, Clone, Copy)]
pub struct BoundingBox {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// A pluggable structural filter that blacks out bounding boxes via the VideoFilter trait.
pub struct BoundingBoxBlackout {
    pub boxes: Vec<BoundingBox>,
    pub margin: u32,
}

// Implement your zero-coupling contract natively
impl VideoFilter for BoundingBoxBlackout {
    fn filter_frame(&self, rgba: &mut [u8], width: u32, height: u32) -> Result<()> {
        let expected = (width * height * 4) as usize;
        if rgba.len() != expected {
            return Err(anyhow!(
                "Buffer size mismatch: expected {} bytes, got {}",
                expected,
                rgba.len()
            ));
        }

        for bbox in &self.boxes {
            // Expand with margin and clamp to frame.
            let margin = self.margin as i32;
            let mut x0 = bbox.x as i32 - margin;
            let mut y0 = bbox.y as i32 - margin;
            let mut x1 = (bbox.x + bbox.w) as i32 + margin;
            let mut y1 = (bbox.y + bbox.h) as i32 + margin;

            x0 = x0.max(0).min(width as i32);
            y0 = y0.max(0).min(height as i32);
            x1 = x1.max(0).min(width as i32);
            y1 = y1.max(0).min(height as i32);

            if x0 >= x1 || y0 >= y1 {
                continue;
            }

            // Runtime check for bounds (debug only).
            debug_assert!(x0 >= 0 && y0 >= 0 && x1 <= width as i32 && y1 <= height as i32);

            // Zero out the region.
            for y in y0..y1 {
                let row_start = ((y * width as i32 + x0) * 4) as usize;
                let row_end = ((y * width as i32 + x1) * 4) as usize;
                for i in (row_start..row_end).step_by(4) {
                    rgba[i] = 0;
                    rgba[i + 1] = 0;
                    rgba[i + 2] = 0;
                    rgba[i + 3] = 255;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blackout_single_box() {
        let mut data = vec![100u8; 4 * 4 * 4];
        let filter = BoundingBoxBlackout {
            boxes: vec![BoundingBox {
                x: 1,
                y: 1,
                w: 2,
                h: 2,
            }],
            margin: 0,
        };
        filter.filter_frame(&mut data, 4, 4).unwrap();
        for y in 0..4 {
            for x in 0..4 {
                let idx = (y * 4 + x) * 4;
                let is_inside = x >= 1 && x <= 2 && y >= 1 && y <= 2;
                let expected = if is_inside { 0 } else { 100 };
                assert_eq!(data[idx], expected);
                assert_eq!(data[idx + 1], expected);
                assert_eq!(data[idx + 2], expected);
                if is_inside {
                    assert_eq!(data[idx + 3], 255);
                } else {
                    assert_eq!(data[idx + 3], 100);
                }
            }
        }
    }

    #[test]
    fn test_blackout_with_margin() {
        let mut data = vec![200u8; 4 * 4 * 4];
        let filter = BoundingBoxBlackout {
            boxes: vec![BoundingBox {
                x: 0,
                y: 0,
                w: 2,
                h: 2,
            }],
            margin: 1,
        };
        filter.filter_frame(&mut data, 4, 4).unwrap();
        // Margin = 1 => black region should be x=0..2, y=0..2 (clamped to 4).
        for y in 0..4 {
            for x in 0..4 {
                let idx = (y * 4 + x) * 4;
                let is_inside = x < 3 && y < 3;
                let expected = if is_inside { 0 } else { 200 };
                assert_eq!(data[idx], expected);
            }
        }
    }
}
