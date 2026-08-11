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
    mask_threshold: f32,
    dilation_kernel_size: u32,
    dilation_iterations: u32,
    mask_buf: Mutex<Vec<u8>>,
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
            ValueType::Tensor { shape, .. } => shape,
            _ => return Err(anyhow!("Expected tensor input")),
        };

        if input_shape.len() != 4 {
            return Err(anyhow!("Expected input tensor of rank 4"));
        }

        // Check batch and channel dimensions: they must be 1 and 3, unless dynamic (-1)
        let batch = input_shape[0];
        if batch != -1 && batch != 1 {
            return Err(anyhow!("Batch dimension must be 1 (or dynamic)"));
        }
        let channels = input_shape[1];
        if channels != -1 && channels != 3 {
            return Err(anyhow!("Channel dimension must be 3 (or dynamic)"));
        }
        // We don't need to store height and width; they will be taken from the frame.

        Ok(Self {
            session: Mutex::new(session),
            input_name,
            output_name,
            mask_threshold: 0.0,
            dilation_kernel_size: 5,
            dilation_iterations: 3,
            mask_buf: Mutex::new(Vec::new()),
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
        let frame_start = Instant::now();
        let orig_w = width as usize;
        let orig_h = height as usize;
        if rgba.len() != orig_w * orig_h * 4 {
            return Err(anyhow!("Buffer size mismatch"));
        }

        // FCN-ResNet50 downsamples by factor 8.
        let latent_w = orig_w / 8;
        let latent_h = orig_h / 8;

        // --- Preprocess: RGBA → RGB, normalise ---
        let pre_start = Instant::now();
        let mut input_data = Vec::with_capacity(3 * orig_h * orig_w);
        for y in 0..orig_h {
            let row = y * orig_w * 4;
            for x in 0..orig_w {
                let idx = row + x * 4;
                input_data.push(rgba[idx] as f32 / 255.0);
                input_data.push(rgba[idx + 1] as f32 / 255.0);
                input_data.push(rgba[idx + 2] as f32 / 255.0);
            }
        }
        let pre_dur = pre_start.elapsed();

        // --- Inference ---
        let inf_start = Instant::now();
        let input_tensor = Value::from_array(([1, 3, orig_h, orig_w], input_data))?;

        let mut session_guard = self.session.lock().unwrap();
        let outputs = session_guard.run(ort::inputs![self.input_name.as_str() => input_tensor])?;
        let output_tensor = outputs
            .get(self.output_name.as_str())
            .ok_or_else(|| anyhow!("Output tensor missing"))?;
        let (_, raw_slice) = output_tensor.try_extract_tensor::<f32>()?;
        let inf_dur = inf_start.elapsed();

        // --- Postprocess: extract class 15 and upscale ---
        let post_start = Instant::now();
        let view = ndarray::ArrayView4::from_shape((1, 21, latent_h, latent_w), raw_slice)
            .map_err(|e| anyhow!("Shape error: {:?}", e))?;

        let mut mask_buf = self.mask_buf.lock().unwrap();
        mask_buf.clear();
        mask_buf.resize(orig_w * orig_h, 0);

        let scale_x = latent_w as f32 / orig_w as f32;
        let scale_y = latent_h as f32 / orig_h as f32;

        for y in 0..orig_h {
            let lat_y = (y as f32 * scale_y) as usize;
            let row_offset = y * orig_w;
            for x in 0..orig_w {
                let lat_x = (x as f32 * scale_x) as usize;
                let confidence = view[[0, 15, lat_y, lat_x]];
                if confidence > self.mask_threshold {
                    mask_buf[row_offset + x] = 1;
                }
            }
        }

        if self.dilation_iterations > 0 {
            *mask_buf = Self::dilate_mask(
                &mask_buf,
                orig_w,
                orig_h,
                self.dilation_kernel_size as usize,
                self.dilation_iterations as usize,
            );
        }

        for y in 0..orig_h {
            let row_offset = y * orig_w;
            for x in 0..orig_w {
                let idx = row_offset + x;
                if mask_buf[idx] == 1 {
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
