// ============================================================================
// ASSET ACKNOWLEDGMENT & ATTRIBUTION:
// This module utilizes the PPHumanSeg (Paddle-to-ONNX) model, licensed under Apache 2.0.
// Source: https://huggingface.co/opencv/human_segmentation_pphumanseg/resolve/main/human_segmentation_pphumanseg_2023mar.onnx
//
// This model replaces previous architectures to bypass custom PyTorch coordinate
// transformations (e.g., pytorch_half_pixel), enabling native, zero-CPU-fallback
// execution on Intel Iris Xe hardware via DirectML/OpenVINO.
// ============================================================================

use crate::filter::VideoFilter;
use anyhow::{Result, anyhow};
use ort::{
    session::Session,
    value::{Value, ValueType},
};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

pub struct PeopleSegFilter {
    session: Mutex<Session>,
    input_name: String,
    output_name: String,
    model_h: usize,
    model_w: usize,
    num_classes: usize,
    mask_threshold: f32,
    dilation_kernel_size: u32,
    dilation_iterations: u32,
}

impl PeopleSegFilter {
    pub fn new(path: &str) -> Result<Self> {
        let session = super::init_session(path)?;

        let input_name = session.inputs()[0].name().to_string();
        let output_name = session.outputs()[0].name().to_string();

        let input_shape = match session.inputs()[0].dtype() {
            ValueType::Tensor { shape, .. } => shape,
            _ => return Err(anyhow!("Expected tensor input")),
        };

        if input_shape.len() != 4 {
            return Err(anyhow!("Expected input tensor of rank 4"));
        }

        let batch = input_shape[0];
        let channels = input_shape[1];
        let height = input_shape[2];
        let width = input_shape[3];

        if batch != 1 {
            return Err(anyhow!("Batch dimension must be 1 (got {})", batch));
        }
        if channels != 3 {
            return Err(anyhow!("Channel dimension must be 3 (got {})", channels));
        }
        if height <= 0 || width <= 0 {
            return Err(anyhow!("Height and width must be positive integers"));
        }

        let model_h = height as usize;
        let model_w = width as usize;

        let output_shape = match session.outputs()[0].dtype() {
            ValueType::Tensor { shape, .. } => shape,
            _ => return Err(anyhow!("Expected tensor output")),
        };
        let num_classes = if output_shape.len() == 4 {
            output_shape[1] as usize
        } else {
            2
        };

        if num_classes < 2 {
            return Err(anyhow!("Expected at least 2 output classes"));
        }

        println!(
            "DEBUG: PPHumanSeg model – input: {}x{}, classes: {}",
            model_h, model_w, num_classes
        );

        Ok(Self {
            session: Mutex::new(session),
            input_name,
            output_name,
            model_h,
            model_w,
            num_classes,
            mask_threshold: 0.5,
            dilation_kernel_size: 3,
            dilation_iterations: 1,
        })
    }

    /// Dilate a binary mask using a square kernel.
    fn dilate_mask(
        mask: &[u8],
        width: usize,
        height: usize,
        kernel_size: usize,
        iterations: usize,
    ) -> Vec<u8> {
        if mask.is_empty() || width == 0 || height == 0 || iterations == 0 {
            return mask.to_vec();
        }
        let mut output = mask.to_vec();
        let half = kernel_size / 2;
        for _ in 0..iterations {
            let mut temp = vec![0u8; width * height];
            for y in 0..height {
                let row_offset = y * width;
                let mut active_ones = 0;
                for x in 0..=half.min(width - 1) {
                    if output[row_offset + x] == 1 {
                        active_ones += 1;
                    }
                }
                for x in 0..width {
                    if active_ones > 0 {
                        temp[row_offset + x] = 1;
                    }
                    if x >= half {
                        if output[row_offset + (x - half)] == 1 {
                            active_ones -= 1;
                        }
                    }
                    if x + half + 1 < width {
                        if output[row_offset + (x + half + 1)] == 1 {
                            active_ones += 1;
                        }
                    }
                }
            }
            let mut new_output = vec![0u8; width * height];
            for x in 0..width {
                let mut active_ones = 0;
                for y in 0..=half.min(height - 1) {
                    if temp[y * width + x] == 1 {
                        active_ones += 1;
                    }
                }
                for y in 0..height {
                    if active_ones > 0 {
                        new_output[y * width + x] = 1;
                    }
                    if y >= half {
                        if temp[(y - half) * width + x] == 1 {
                            active_ones -= 1;
                        }
                    }
                    if y + half + 1 < height {
                        if temp[(y + half + 1) * width + x] == 1 {
                            active_ones += 1;
                        }
                    }
                }
            }
            output = new_output;
        }
        output
    }

    /// Fast bilinear upscale of a binary mask from low-res to target resolution.
    /// Outputs `mask_out` of size `target_w * target_h` (u8, 0/1).
    fn bilinear_upscale(
        mask_low: &[u8],
        low_w: usize,
        low_h: usize,
        target_w: usize,
        target_h: usize,
        mask_out: &mut [u8],
    ) {
        let scale_x = low_w as f32 / target_w as f32;
        let scale_y = low_h as f32 / target_h as f32;

        for y in 0..target_h {
            let src_y_float = (y as f32 + 0.5) * scale_y - 0.5;
            let src_y = src_y_float.clamp(0.0, (low_h - 1) as f32).floor() as usize;
            let src_y_next = (src_y + 1).min(low_h - 1);
            let frac_y = src_y_float - src_y as f32;

            let row_base = y * target_w;
            let src_row1 = src_y * low_w;
            let src_row2 = src_y_next * low_w;

            for x in 0..target_w {
                let src_x_float = (x as f32 + 0.5) * scale_x - 0.5;
                let src_x = src_x_float.clamp(0.0, (low_w - 1) as f32).floor() as usize;
                let src_x_next = (src_x + 1).min(low_w - 1);
                let frac_x = src_x_float - src_x as f32;

                let v00 = mask_low[src_row1 + src_x] as f32;
                let v10 = mask_low[src_row1 + src_x_next] as f32;
                let v01 = mask_low[src_row2 + src_x] as f32;
                let v11 = mask_low[src_row2 + src_x_next] as f32;

                let val = v00 * (1.0 - frac_x) * (1.0 - frac_y)
                    + v10 * frac_x * (1.0 - frac_y)
                    + v01 * (1.0 - frac_x) * frac_y
                    + v11 * frac_x * frac_y;
                mask_out[row_base + x] = if val > 0.5 { 1 } else { 0 };
            }
        }
    }
}

impl VideoFilter for PeopleSegFilter {
    fn filter_frame(&self, rgba: &mut [u8], width: u32, height: u32) -> Result<()> {
        let frame_start = Instant::now();
        let orig_w = width as usize;
        let orig_h = height as usize;
        if rgba.len() != orig_w * orig_h * 4 {
            return Err(anyhow!("Buffer size mismatch"));
        }

        let model_h = self.model_h;
        let model_w = self.model_w;
        let plane_stride = model_h * model_w;

        // --- 1. Preprocessing: box‑filter downscale + planar RGB f32 ---
        let pre_start = Instant::now();
        let mut input_buf = vec![0.0f32; 3 * plane_stride];

        let block_w = orig_w / model_w; // exactly 5
        let block_h_exact = orig_h as f32 / model_h as f32; // ~2.8125

        for y_out in 0..model_h {
            let start_y = (y_out as f32 * block_h_exact) as usize;
            let end_y = (((y_out + 1) as f32 * block_h_exact) as usize).min(orig_h);
            let actual_h = end_y - start_y;
            let cell_pixel_count = (block_w * actual_h) as f32;

            for x_out in 0..model_w {
                let start_x = x_out * block_w;
                let end_x = (start_x + block_w).min(orig_w);

                let mut sum_r = 0u32;
                let mut sum_g = 0u32;
                let mut sum_b = 0u32;

                for sy in start_y..end_y {
                    let row_offset = sy * orig_w * 4;
                    for sx in start_x..end_x {
                        let idx = row_offset + sx * 4;
                        sum_r += rgba[idx] as u32;
                        sum_g += rgba[idx + 1] as u32;
                        sum_b += rgba[idx + 2] as u32;
                    }
                }

                let target_idx = y_out * model_w + x_out;
                let denom = cell_pixel_count * 255.0;
                input_buf[target_idx] = sum_r as f32 / denom;
                input_buf[plane_stride + target_idx] = sum_g as f32 / denom;
                input_buf[2 * plane_stride + target_idx] = sum_b as f32 / denom;
            }
        }

        let pre_dur = pre_start.elapsed();

        // --- 2. Inference ---
        let inf_start = Instant::now();
        let input_tensor = Value::from_array(([1, 3, model_h, model_w], input_buf))?;

        let mut session_guard = self.session.lock().unwrap();
        let outputs = session_guard.run(ort::inputs![self.input_name.as_str() => input_tensor])?;
        let output_tensor = outputs
            .get(self.output_name.as_str())
            .ok_or_else(|| anyhow!("Output tensor missing"))?;
        let (_, raw_slice) = output_tensor.try_extract_tensor::<f32>()?;
        let inf_dur = inf_start.elapsed();

        // --- 3. Postprocess: extract class 1, low‑res dilation, bilinear upscale ---
        let post_start = Instant::now();

        let view =
            ndarray::ArrayView4::from_shape((1, self.num_classes, model_h, model_w), raw_slice)
                .map_err(|e| anyhow!("Shape error: {:?}", e))?;

        // Create low‑res mask from class 1
        let mut mask_low = vec![0u8; plane_stride];
        for y in 0..model_h {
            let row = y * model_w;
            for x in 0..model_w {
                let confidence = view[[0, 1, y, x]];
                mask_low[row + x] = if confidence > self.mask_threshold {
                    1
                } else {
                    0
                };
            }
        }

        // Apply dilation on low‑res mask
        let mask_low_dilated = if self.dilation_iterations > 0 && self.dilation_kernel_size > 1 {
            Self::dilate_mask(
                &mask_low,
                model_w,
                model_h,
                self.dilation_kernel_size as usize,
                self.dilation_iterations as usize,
            )
        } else {
            mask_low
        };

        // Upscale to original resolution with bilinear
        let mut mask_full = vec![0u8; orig_w * orig_h];
        Self::bilinear_upscale(
            &mask_low_dilated,
            model_w,
            model_h,
            orig_w,
            orig_h,
            &mut mask_full,
        );

        // Apply blackout
        for y in 0..orig_h {
            let row_offset = y * orig_w;
            for x in 0..orig_w {
                let idx = row_offset + x;
                if mask_full[idx] == 1 {
                    let px = idx * 4;
                    rgba[px] = 0;
                    rgba[px + 1] = 0;
                    rgba[px + 2] = 0;
                    rgba[px + 3] = 255;
                }
            }
        }

        let post_dur = post_start.elapsed();
        let total_dur = frame_start.elapsed();

        // --- Profile output ---
        static FRAME_COUNT: AtomicUsize = AtomicUsize::new(0);
        let count = FRAME_COUNT.fetch_add(1, Ordering::Relaxed);
        if count % 1 == 0 {
            println!(
                "[PROFILE] Frame {}: Pre={:.2}ms, Inf={:.2}ms, Post={:.2}ms, Total={:.2}ms",
                count,
                pre_dur.as_secs_f64() * 1000.0,
                inf_dur.as_secs_f64() * 1000.0,
                post_dur.as_secs_f64() * 1000.0,
                total_dur.as_secs_f64() * 1000.0,
            );
        }

        Ok(())
    }
}
