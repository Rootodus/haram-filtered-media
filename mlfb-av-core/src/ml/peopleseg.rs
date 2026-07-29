// ============================================================================
// ASSET ACKNOWLEDGMENT & ATTRIBUTION:
// This module executes inference against an optimized, standalone UNet weight
// binary acquired from the open-source community.
//
// Source Project: PINTO Model Zoo
// Asset Model Target: 466_People_Segmentation (UNet Architecture)
// Curator/Converter: Katsuya Hyodo (pinto0309)
// Source Repository: https://github.com/PINTO0309/PINTO_model_zoo
// Upstream Model Research: Vladimir Iglovikov (people_segmentation)
// Upstream Asset License: MIT
//
// NOTE: The real-time pixel normalization, image resizing, and 4D tensor
// indexing layout maps implemented below are custom native Rust pipeline
// components written to interface directly with the standalone ONNX matrix.
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

        let output_shape = match session.outputs()[0].dtype() {
            ValueType::Tensor { shape, .. } => shape.clone(),
            _ => return Err(anyhow!("Expected tensor output")),
        };

        // ====================================================================
        // THE SANITY CHECK: Print the actual hidden output dimensions
        // ====================================================================
        println!(
            "DEBUG CRITICAL: True Model Output Shape is: {:?}",
            output_shape
        );
        // ====================================================================

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
            mask_threshold: 0.0,
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
                    // FIX: Scale raw 0-255 u8 values down to 0.0-1.0 f32 range
                    let raw_pixel = dst_data[(y * model_w + x) * 4 + ch] as f32;
                    input_data.push(raw_pixel / 255.0);
                }
            }
        }

        // --- Core Model Inference Loop ---
        let input_tensor = Value::from_array(([1, 3, model_h, model_w], input_data))?;

        let mut session_guard = self
            .session
            .lock()
            .map_err(|_| anyhow!("Failed to acquire safe lock on ONNX session state"))?;

        let outputs = session_guard.run(ort::inputs![self.input_name.as_str() => input_tensor])?;
        let output_tensor = outputs
            .get(self.output_name.as_str())
            .ok_or_else(|| anyhow!("Output tensor missing"))?;
        let (_, raw_slice) = output_tensor.try_extract_tensor::<f32>()?;

        // --- 1. Re-interpret flat FFI float array as a 4D viewport ---
        // FIX 1: Change the shape parameters to match your true [1, 1, 384, 640] layout
        let view4d = ndarray::ArrayView4::from_shape((1, 1, model_h, model_w), raw_slice)
            .map_err(|e| anyhow!("Array layout transposition error: {:?}", e))?;

        let mut combined_mask = vec![0u8; orig_width * orig_height];
        for y in 0..orig_height {
            let norm_y = y as f32 / orig_height as f32;
            let model_y = ((norm_y * model_h as f32) as usize).min(model_h - 1);
            let row_offset = y * orig_width;

            for x in 0..orig_width {
                let norm_x = x as f32 / orig_width as f32;
                let model_x = ((norm_x * model_w as f32) as usize).min(model_w - 1);

                // FIX 2: Target channel index 0 natively (since there is only one layer)
                let confidence = view4d[[0, 0, model_y, model_x]];

                // Keep the raw logit threshold filter boundary (> 0.0)
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
