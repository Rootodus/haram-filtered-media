// ============================================================================
// ATTRIBUTION NOTICE:
// The mathematical tensor stride indexing, channel mapping,
// and post-processing logic implemented in this module are translated
// and adapted from the upstream Python reference implementation:
//
// Source Project: PINTO Model Zoo - 466_People_Segmentation
// Author: Katsuya Hyodo (pinto0309)
// Source URL: https://github.com/PINTO0309/PINTO_model_zoo/tree/main/466_People_Segmentation
// Upstream License: MIT
// ============================================================================

use crate::filter::VideoFilter;
use anyhow::{Result, anyhow};
use fast_image_resize::{FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer, images::Image};
use ort::{
    session::Session,
    value::{Value, ValueType},
};
use std::sync::Mutex;

pub struct PeopleSegFilter {
    session: Mutex<Session>,
    input_name: String,
    output_name: String,
    pub input_height: u32,
    pub input_width: u32,
    mask_threshold: f32,
    dilation_kernel_size: u32,
    dilation_iterations: u32,
}

impl PeopleSegFilter {
    pub fn new(path: &str) -> Result<Self> {
        // Direct clean call to parent module's initialization engine
        let session = super::init_session(path)?;

        let input_name = session.inputs()[0].name().to_string();
        let output_names: Vec<String> = session
            .outputs()
            .iter()
            .map(|o| o.name().to_string())
            .collect();

        if output_names.is_empty() {
            return Err(anyhow!("Expected at least 1 output tensor, got 0"));
        }
        let output_name = output_names[0].clone();

        let input_shape = match session.inputs()[0].dtype() {
            ValueType::Tensor { shape, .. } => shape.clone(),
            _ => return Err(anyhow!("Expected tensor input")),
        };

        if input_shape.len() != 4 || input_shape[0] != 1 || input_shape[1] != 3 {
            return Err(anyhow!("Expected input shape (1, 3, H, W)"));
        }

        let input_height = input_shape[2] as u32;
        let input_width = input_shape[3] as u32;

        Ok(Self {
            session: Mutex::new(session),
            input_name,
            output_name,
            input_height,
            input_width,
            mask_threshold: 0.5,
            dilation_kernel_size: 5,
            dilation_iterations: 3,
        })
    }

    /// Dilate a binary mask using a square kernel. (Isolated helper)
    fn dilate_mask(
        mask: &[u8],
        width: usize,
        height: usize,
        kernel_size: usize,
        iterations: usize,
    ) -> Vec<u8> {
        if mask.is_empty() || width == 0 || height == 0 {
            return vec![];
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
}

impl VideoFilter for PeopleSegFilter {
    fn filter_frame(&self, rgba: &mut [u8], width: u32, height: u32) -> Result<()> {
        let orig_width = width as usize;
        let orig_height = height as usize;
        if rgba.len() != orig_width * orig_height * 4 {
            return Err(anyhow!("Buffer size mismatch"));
        }

        let model_w = self.input_width as usize;
        let model_h = self.input_height as usize;

        // --- Preprocessing: Image Resize ---
        let src_image = Image::from_slice_u8(width, height, rgba, PixelType::U8x4)?;
        let mut dst_rgba = Image::new(model_w as u32, model_h as u32, PixelType::U8x4);
        let mut resizer = Resizer::new();
        let options = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Bilinear));
        resizer.resize(&src_image, &mut dst_rgba, Some(&options))?;

        let mut input_data = Vec::with_capacity(3 * model_h * model_w);
        let dst_data = dst_rgba.buffer();
        for ch in 0..3 {
            for y in 0..model_h {
                for x in 0..model_w {
                    input_data.push(dst_data[(y * model_w + x) * 4 + ch] as f32);
                }
            }
        }

        // --- Core Model Inference Loop ---
        // --- Core Model Inference Loop ---
        let input_tensor = Value::from_array(([1, 3, model_h, model_w], input_data))?;

        // FIX: Acquire a local mutable guard block from the interior Mutex container
        let mut session_guard = self
            .session
            .lock()
            .map_err(|_| anyhow!("Failed to acquire safe lock on ONNX session state"))?;

        // Call run on your mutable guard variable reference instead of directly on self.session
        let outputs = session_guard.run(ort::inputs![self.input_name.as_str() => input_tensor])?;

        let output_tensor = outputs
            .get(self.output_name.as_str())
            .ok_or_else(|| anyhow!("Output tensor missing"))?;

        let (_, raw_slice) = output_tensor.try_extract_tensor::<f32>()?;

        // --- Output Bitmask Assignment via Safe 4D ndarray View ---
        // Expected shape signature from folder 466 outputs is [1, 2, model_h, model_w]
        let view4d = ndarray::ArrayView4::from_shape((1, 2, model_h, model_w), raw_slice)
            .map_err(|e| anyhow!("Array layout transposition error: {:?}", e))?;

        let mut combined_mask = vec![0u8; orig_width * orig_height];
        for y in 0..orig_height {
            let norm_y = y as f32 / orig_height as f32;
            let model_y = ((norm_y * model_h as f32) as usize).min(model_h - 1);
            let row_offset = y * orig_width;

            for x in 0..orig_width {
                let norm_x = x as f32 / orig_width as f32;
                let model_x = ((norm_x * model_w as f32) as usize).min(model_w - 1);

                // Zero-copy index lookup targeting Channel 1 (Human Silhouette probabilities) exclusively
                let confidence = view4d[[0, 1, model_y, model_x]];
                if confidence > self.mask_threshold {
                    combined_mask[row_offset + x] = 1;
                }
            }
        }

        // --- Mask Dilation & Blackout Pass ---
        if self.dilation_iterations > 0 {
            combined_mask = Self::dilate_mask(
                &combined_mask,
                orig_width,
                orig_height,
                self.dilation_kernel_size as usize,
                self.dilation_iterations as usize,
            );
        }

        for y in 0..orig_height {
            let row_offset = y * orig_width;
            for x in 0..orig_width {
                let idx = row_offset + x;
                if combined_mask[idx] == 1 {
                    let px = idx * 4;
                    rgba[px] = 0;
                    rgba[px + 1] = 0;
                    rgba[px + 2] = 0;
                    rgba[px + 3] = 255;
                }
            }
        }

        Ok(())
    }
}
