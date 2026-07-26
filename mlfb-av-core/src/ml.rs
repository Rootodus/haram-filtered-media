use anyhow::{Result, anyhow};
use fast_image_resize::{FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer, images::Image};
use ort::{
    execution_providers::CPUExecutionProvider,
    session::Session,
    session::builder::GraphOptimizationLevel,
    value::{Value, ValueType},
};

pub struct VideoModel {
    session: Session,
    input_name: String,
    output_name: String,
    input_height: u32,
    input_width: u32,
}

impl VideoModel {
    pub fn new(path: &str) -> Result<Self> {
        let session = Session::builder()
            .map_err(|e| anyhow!("Failed to create session builder: {}", e))?
            .with_optimization_level(GraphOptimizationLevel::Level1)
            .map_err(|e| anyhow!("Failed to set optimization level: {}", e))?
            .with_intra_threads(1)
            .map_err(|e| anyhow!("Failed to set intra threads: {}", e))?
            .with_execution_providers([CPUExecutionProvider::default().build()])
            .map_err(|e| anyhow!("Failed to set execution provider: {}", e))?
            .commit_from_file(path)
            .map_err(|e| anyhow!("Failed to load model file: {}", e))?;

        let input_name = session.inputs()[0].name().to_string();
        let output_name = session.outputs()[0].name().to_string();

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
            session,
            input_name,
            output_name,
            input_height,
            input_width,
        })
    }

    pub fn process(&mut self, rgba: &mut [u8], width: u32, height: u32) -> Result<()> {
        let orig_width = width as usize;
        let orig_height = height as usize;
        let expected_size = orig_width * orig_height * 4;
        if rgba.len() != expected_size {
            return Err(anyhow!("Buffer size mismatch"));
        }

        let model_w = self.input_width as usize;
        let model_h = self.input_height as usize;

        // 1. Resize RGBA to model input size.
        let src_image = Image::from_slice_u8(width, height, rgba, PixelType::U8x4)?;
        let mut dst_rgba = Image::new(model_w as u32, model_h as u32, PixelType::U8x4);
        let mut resizer = Resizer::new();
        let options = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Lanczos3));
        resizer.resize(&src_image, &mut dst_rgba, Some(&options))?;

        // 2. Preprocess: RGBA -> RGB -> NCHW f32.
        let mut input_data = Vec::with_capacity(3 * model_h * model_w);
        let dst_data = dst_rgba.buffer();
        for ch in 0..3 {
            for y in 0..model_h {
                for x in 0..model_w {
                    let idx = (y * model_w + x) * 4;
                    input_data.push(dst_data[idx + ch] as f32);
                }
            }
        }

        // 3. Run inference.
        let input_tensor = Value::from_array(([1, 3, model_h, model_w], input_data))?;
        let outputs = self
            .session
            .run(ort::inputs![self.input_name.as_str() => input_tensor])?;

        let output_value = outputs
            .get(self.output_name.as_str())
            .ok_or_else(|| anyhow!("Output name not found"))?;
        let (_shape, output_data) = output_value.try_extract_tensor::<f32>()?;

        // 4. Postprocess: output -> RGB u8 at model size.
        let mut rgb_resized = vec![0u8; model_w * model_h * 3];
        for y in 0..model_h {
            for x in 0..model_w {
                let idx = (y * model_w + x) * 3;
                rgb_resized[idx] =
                    (output_data[0 * model_h * model_w + y * model_w + x].clamp(0.0, 255.0)) as u8;
                rgb_resized[idx + 1] =
                    (output_data[1 * model_h * model_w + y * model_w + x].clamp(0.0, 255.0)) as u8;
                rgb_resized[idx + 2] =
                    (output_data[2 * model_h * model_w + y * model_w + x].clamp(0.0, 255.0)) as u8;
            }
        }

        // 5. Resize RGB back to original size.
        let src_rgb = Image::from_slice_u8(
            model_w as u32,
            model_h as u32,
            &mut rgb_resized[..],
            PixelType::U8x3,
        )?;
        let mut dst_rgb = Image::new(width, height, PixelType::U8x3);
        resizer.resize(&src_rgb, &mut dst_rgb, Some(&options))?;

        // 6. Copy RGB to RGBA with alpha=255.
        let rgb_data = dst_rgb.buffer();
        for y in 0..orig_height {
            for x in 0..orig_width {
                let idx = (y * orig_width + x) * 3;
                let idx4 = (y * orig_width + x) * 4;
                rgba[idx4] = rgb_data[idx];
                rgba[idx4 + 1] = rgb_data[idx + 1];
                rgba[idx4 + 2] = rgb_data[idx + 2];
                rgba[idx4 + 3] = 255;
            }
        }

        Ok(())
    }
}
