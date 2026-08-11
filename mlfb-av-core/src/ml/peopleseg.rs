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
use fast_image_resize::{FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer, images::Image};
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
    model_height: u32,
    model_width: u32,
    num_classes: u32, // typically 2 (background, person)
    mask_threshold: f32,
    dilation_kernel_size: u32,
    dilation_iterations: u32,
    mask_buf: Mutex<Vec<u8>>,
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

        // We expect static dimensions: [1, 3, H, W]
        let batch = input_shape[0];
        let channels = input_shape[1];
        let height = input_shape[2];
        let width = input_shape[3];

        if batch != 1 {
            return Err(anyhow!("Batch dimension must be 1"));
        }
        if channels != 3 {
            return Err(anyhow!("Channel dimension must be 3"));
        }
        if height <= 0 || width <= 0 {
            return Err(anyhow!("Height and width must be positive integers"));
        }

        let model_height = height as u32;
        let model_width = width as u32;

        // Get output shape to know number of classes
        let output_shape = match session.outputs()[0].dtype() {
            ValueType::Tensor { shape, .. } => shape,
            _ => return Err(anyhow!("Expected tensor output")),
        };
        let num_classes = if output_shape.len() == 4 {
            output_shape[1] as u32
        } else {
            2 // fallback
        };

        println!(
            "DEBUG: Model input: {}x{}, output classes: {}",
            model_height, model_width, num_classes
        );

        Ok(Self {
            session: Mutex::new(session),
            input_name,
            output_name,
            model_height,
            model_width,
            num_classes,
            mask_threshold: 0.5,
            dilation_kernel_size: 5,
            dilation_iterations: 3,
            mask_buf: Mutex::new(Vec::new()),
        })
    }

    /// Dilate a binary mask using a square kernel. (Unchanged)
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

        let model_h = self.model_height as usize;
        let model_w = self.model_width as usize;

        // --- Preprocess: downscale RGBA to model size, then convert to RGB ---
        let pre_start = Instant::now();

        // 1. Downscale using fast_image_resize (cheap because target is small)
        let src_image = Image::from_slice_u8(width, height, rgba, PixelType::U8x4)?;
        let mut dst_rgba = Image::new(model_w as u32, model_h as u32, PixelType::U8x4);
        let mut resizer = Resizer::new();
        let options = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Bilinear));
        resizer.resize(&src_image, &mut dst_rgba, Some(&options))?;

        // 2. Convert RGBA to RGB and normalise to [0,1]
        let mut input_data = Vec::with_capacity(3 * model_h * model_w);
        let dst_data = dst_rgba.buffer();
        for ch in 0..3 {
            for y in 0..model_h {
                let row = y * model_w;
                for x in 0..model_w {
                    let idx = (row + x) * 4 + ch;
                    input_data.push(dst_data[idx] as f32 / 255.0);
                }
            }
        }
        let pre_dur = pre_start.elapsed();

        // --- Inference ---
        let inf_start = Instant::now();
        let input_tensor = Value::from_array(([1, 3, model_h, model_w], input_data))?;

        let mut session_guard = self.session.lock().unwrap();
        let outputs = session_guard.run(ort::inputs![self.input_name.as_str() => input_tensor])?;
        let output_tensor = outputs
            .get(self.output_name.as_str())
            .ok_or_else(|| anyhow!("Output tensor missing"))?;
        let (_, raw_slice) = output_tensor.try_extract_tensor::<f32>()?;
        let inf_dur = inf_start.elapsed();

        // --- Postprocess: extract class 1 (person) and upscale to original ---
        let post_start = Instant::now();

        // Interpret output as [1, num_classes, model_h, model_w]
        let view = ndarray::ArrayView4::from_shape(
            (1, self.num_classes as usize, model_h, model_w),
            raw_slice,
        )
        .map_err(|e| anyhow!("Shape error: {:?}", e))?;

        let mut mask_buf = self.mask_buf.lock().unwrap();
        mask_buf.clear();
        mask_buf.resize(orig_w * orig_h, 0);

        // Nearest-neighbour upscale from model_h/model_w to orig_h/orig_w
        let scale_x = model_w as f32 / orig_w as f32;
        let scale_y = model_h as f32 / orig_h as f32;

        // We assume class index 1 is "person" (0 = background)
        let person_class = 1; // adjust if needed

        for y in 0..orig_h {
            let model_y = (y as f32 * scale_y) as usize;
            let row_offset = y * orig_w;
            for x in 0..orig_w {
                let model_x = (x as f32 * scale_x) as usize;
                let confidence = view[[0, person_class, model_y, model_x]];
                if confidence > self.mask_threshold {
                    mask_buf[row_offset + x] = 1;
                }
            }
        }

        // Optional dilation
        if self.dilation_iterations > 0 {
            *mask_buf = Self::dilate_mask(
                &mask_buf,
                orig_w,
                orig_h,
                self.dilation_kernel_size as usize,
                self.dilation_iterations as usize,
            );
        }

        // Apply blackout
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
