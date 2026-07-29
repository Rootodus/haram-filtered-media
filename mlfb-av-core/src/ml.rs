use anyhow::{Result, anyhow};
use fast_image_resize::{FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer, images::Image};
use ort::{
    session::Session,
    session::builder::GraphOptimizationLevel,
    value::{Value, ValueType},
};

pub struct VideoModel {
    session: Session,
    input_name: String,
    output_name: String,
    pub input_height: u32,
    pub input_width: u32,
    mask_threshold: f32,
    dilation_kernel_size: u32,
    dilation_iterations: u32,
}

impl VideoModel {
    pub fn new(path: &str) -> Result<Self> {
        // Clean delegation to the private cross-platform initialization module
        let session = Self::init_session(path)?;

        let input_name = session.inputs()[0].name().to_string();

        let output_names: Vec<String> = session
            .outputs()
            .iter()
            .map(|o| o.name().to_string())
            .collect();

        if output_names.is_empty() {
            return Err(anyhow!("Expected at least 1 output tensor, got 0"));
        }
        let output_name = output_names[0].clone(); // Maps your single 2D pixel mask channel

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
            mask_threshold: 0.5,
            dilation_kernel_size: 5,
            dilation_iterations: 3,
        })
    }

    /// Handles cross-platform hardware acceleration backend assignment.
    /// Safely cleans up internal C++ pointers to bypass Send/Sync compilation errors.
    fn init_session(path: &str) -> Result<Session> {
        println!("Instantiating ONNX runtime execution providers...");

        // --- 1. APPLE SILICON TRACK ---
        #[cfg(target_vendor = "apple")]
        {
            use ort::ep::CoreMLExecutionProvider;

            let mut builder = Session::builder()
                .map_err(|e| anyhow!("Failed to create session builder: {}", e))?;
            builder = builder
                .with_optimization_level(GraphOptimizationLevel::Level1)
                .map_err(|e| anyhow!("Failed to set optimization level: {:?}", e))?;
            builder = builder
                .with_intra_threads(1)
                .map_err(|e| anyhow!("Failed to set intra threads: {:?}", e))?;
            builder = builder
                .with_execution_providers([CoreMLExecutionProvider::default().build()])
                .map_err(|e| anyhow!("Failed to set CoreML provider: {:?}", e))?;

            match builder.commit_from_file(path) {
                Ok(s) => {
                    println!(
                        "SUCCESS: CoreML (Apple Silicon NPU/Metal) hardware backend is active."
                    );
                    return Ok(s);
                }
                Err(e) => {
                    println!(
                        "CoreML initialization failed: {}. Falling back to CPU...",
                        e
                    );
                }
            }
        }

        // --- 2. WINDOWS TRACK ---
        #[cfg(target_os = "windows")]
        {
            use ort::ep::{DirectMLExecutionProvider, OpenVINOExecutionProvider};

            // Step A: Attempt Intel OpenVINO iGPU hardware target compilation
            let mut ov_builder = Session::builder()
                .map_err(|e| anyhow!("Failed to create session builder: {}", e))?;
            ov_builder = ov_builder
                .with_optimization_level(GraphOptimizationLevel::Level1)
                .map_err(|e| anyhow!("Failed to set OpenVINO optimization level: {:?}", e))?;
            ov_builder = ov_builder
                .with_intra_threads(1)
                .map_err(|e| anyhow!("Failed to set intra threads: {:?}", e))?;
            ov_builder = ov_builder
                .with_execution_providers([OpenVINOExecutionProvider::default()
                    .with_device_type("GPU")
                    .build()])
                .map_err(|e| anyhow!("Failed to set OpenVINO provider: {:?}", e))?;

            match ov_builder.commit_from_file(path) {
                Ok(s) => {
                    println!("SUCCESS: Intel OpenVINO iGPU hardware backend is active.");
                    return Ok(s);
                }
                Err(_) => {
                    println!("Intel OpenVINO not found or failed. Attempting DirectML...");

                    // Step B: Fall back to DirectML for AMD / NVIDIA architecture support
                    let mut dml_builder = Session::builder()
                        .map_err(|e| anyhow!("Failed to create DirectML builder: {}", e))?;
                    dml_builder = dml_builder
                        .with_optimization_level(GraphOptimizationLevel::Level1)
                        .map_err(|e| {
                            anyhow!("Failed to set DirectML optimization level: {:?}", e)
                        })?;
                    dml_builder = dml_builder
                        .with_intra_threads(1)
                        .map_err(|e| anyhow!("Failed to set intra threads: {:?}", e))?;
                    dml_builder = dml_builder
                        .with_execution_providers([DirectMLExecutionProvider::default().build()])
                        .map_err(|e| anyhow!("Failed to set DirectML provider: {:?}", e))?;

                    match dml_builder.commit_from_file(path) {
                        Ok(s) => {
                            println!(
                                "SUCCESS: DirectML (AMD/NVIDIA/Generic Windows iGPU) hardware backend is active."
                            );
                            return Ok(s);
                        }
                        Err(e) => {
                            println!(
                                "DirectML failed to initialize: {}. Dropping down to raw CPU...",
                                e
                            );
                        }
                    }
                }
            }
        }

        // --- 3. LINUX TRACK ---
        #[cfg(target_os = "linux")]
        {
            use ort::ep::OpenVINOExecutionProvider;

            let mut ov_builder = Session::builder()
                .map_err(|e| anyhow!("Failed to create session builder: {}", e))?;
            ov_builder = ov_builder
                .with_optimization_level(GraphOptimizationLevel::Level1)
                .map_err(|e| anyhow!("Failed to set OpenVINO optimization level: {:?}", e))?;
            ov_builder = ov_builder
                .with_intra_threads(1)
                .map_err(|e| anyhow!("Failed to set intra threads: {:?}", e))?;
            ov_builder = ov_builder
                .with_execution_providers([OpenVINOExecutionProvider::default()
                    .with_device_type("GPU")
                    .build()])
                .map_err(|e| anyhow!("Failed to set OpenVINO provider: {:?}", e))?;

            match ov_builder.commit_from_file(path) {
                Ok(s) => {
                    println!("SUCCESS: Linux Intel OpenVINO iGPU hardware backend is active.");
                    return Ok(s);
                }
                Err(e) => {
                    println!(
                        "OpenVINO hardware init failed: {}. Falling back to standard Linux CPU...",
                        e
                    );
                }
            }
        }

        // --- 4. UNIVERSAL RAW CPU SAFETY FALLBACK ---
        // Serves as ultimate fallback for unhandled OS contexts or failed hardware hooks
        let mut cpu_builder = Session::builder()
            .map_err(|e| anyhow!("Failed to create CPU fallback builder: {}", e))?;
        cpu_builder = cpu_builder
            .with_optimization_level(GraphOptimizationLevel::Level1)
            .map_err(|e| anyhow!("Failed to set CPU optimization level: {:?}", e))?;
        cpu_builder = cpu_builder
            .with_intra_threads(1)
            .map_err(|e| anyhow!("Failed to set intra threads: {:?}", e))?;
        cpu_builder = cpu_builder
            .with_execution_providers([ort::ep::CPUExecutionProvider::default().build()])
            .map_err(|e| anyhow!("Failed to set CPU provider: {:?}", e))?;

        let session = cpu_builder
            .commit_from_file(path)
            .map_err(|e| anyhow!("Critical: CPU fallback compilation crashed: {}", e))?;

        println!("SUCCESS: Standard CPU processing backend is active.");
        Ok(session)
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
                    let idx = (y * model_w + x) * 4;
                    input_data.push(dst_data[idx + ch] as f32);
                }
            }
        }

        // --- Core Model Inference Loop ---
        let input_tensor = Value::from_array(([1, 3, model_h, model_w], input_data))?;
        let outputs = self
            .session
            .run(ort::inputs![self.input_name.as_str() => input_tensor])?;

        // --- Extract the Single 1D Segmentation Float Tensor ---
        let output_tensor = outputs
            .get(self.output_name.as_str())
            .ok_or_else(|| anyhow!("Segmentation output tensor not found"))?;
        let (_, raw_slice) = output_tensor.try_extract_tensor::<f32>()?;

        // --- Apply Output Bitmask to Blackout Buffers ---
        let mut combined_mask = vec![0u8; orig_width * orig_height];

        for y in 0..orig_height {
            let norm_y = y as f32 / orig_height as f32;
            let model_y = ((norm_y * model_h as f32) as usize).min(model_h - 1);
            let row_offset = y * orig_width;

            for x in 0..orig_width {
                let norm_x = x as f32 / orig_width as f32;
                let model_x = ((norm_x * model_w as f32) as usize).min(model_w - 1);

                // Safe, linear pixel array lookup
                let confidence = raw_slice[model_y * model_w + model_x];

                if confidence > self.mask_threshold {
                    combined_mask[row_offset + x] = 1;
                }
            }
        }

        // --- Perform Structural Mask Dilation ---
        if self.dilation_iterations > 0 {
            combined_mask = Self::dilate_mask(
                &combined_mask,
                orig_width,
                orig_height,
                self.dilation_kernel_size as usize,
                self.dilation_iterations as usize,
            );
        }

        // --- Execute Blackout Pixel Transformations ---
        for y in 0..orig_height {
            let row_offset = y * orig_width;
            for x in 0..orig_width {
                let idx = row_offset + x;
                if combined_mask[idx] == 1 {
                    let px = idx * 4;
                    rgba[px] = 0; // R
                    rgba[px + 1] = 0; // G
                    rgba[px + 2] = 0; // B
                    rgba[px + 3] = 255; // A
                }
            }
        }

        Ok(())
    }

    /// Dilate a binary mask using a square kernel.
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
            // --- PASS 1: HORIZONTAL SLIDING WINDOW ---
            let mut temp = vec![0u8; width * height];
            for y in 0..height {
                let row_offset = y * width;
                let mut active_ones = 0;

                // Initialize the window for the start of the row
                for x in 0..=half.min(width - 1) {
                    if output[row_offset + x] == 1 {
                        active_ones += 1;
                    }
                }

                // Slide across the row
                for x in 0..width {
                    if active_ones > 0 {
                        temp[row_offset + x] = 1;
                    }

                    // Element leaving the window (left side)
                    if x >= half {
                        let out_x = x - half;
                        if output[row_offset + out_x] == 1 {
                            active_ones -= 1;
                        }
                    }
                    // Element entering the window (right side)
                    let in_x = x + half + 1;
                    if in_x < width {
                        if output[row_offset + in_x] == 1 {
                            active_ones += 1;
                        }
                    }
                }
            }

            // --- PASS 2: VERTICAL SLIDING WINDOW ---
            let mut new_output = vec![0u8; width * height];
            for x in 0..width {
                let mut active_ones = 0;

                // Initialize the window for the start of the column
                for y in 0..=half.min(height - 1) {
                    if temp[y * width + x] == 1 {
                        active_ones += 1;
                    }
                }

                // Slide down the column
                for y in 0..height {
                    if active_ones > 0 {
                        new_output[y * width + x] = 1;
                    }

                    // Element leaving the window (top side)
                    if y >= half {
                        let out_y = y - half;
                        if temp[out_y * width + x] == 1 {
                            active_ones -= 1;
                        }
                    }
                    // Element entering the window (bottom side)
                    let in_y = y + half + 1;
                    if in_y < height {
                        if temp[in_y * width + x] == 1 {
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
