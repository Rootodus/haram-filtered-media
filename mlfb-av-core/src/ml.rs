use crate::detection::BoundingBox;
use anyhow::{Result, anyhow};
use edgefirst_decoder::yolo::FloatProtoElem;
use edgefirst_decoder::{DecoderBuilder, DetectBox, Segmentation};
use fast_image_resize::{FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer, images::Image};
use ort::session::Session;
use ort::session::builder::GraphOptimizationLevel;
use ort::value::{Value, ValueType};

pub struct VideoModel {
    session: Session,
    input_name: String,
    box_output_name: String,
    proto_output_name: String,
    input_height: u32,
    input_width: u32,
    confidence_threshold: f32,
    nms_threshold: f32,
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

        if output_names.len() < 2 {
            return Err(anyhow!(
                "Expected at least 2 outputs, got {}",
                output_names.len()
            ));
        }

        let box_output_name = output_names[0].clone(); // "output0"
        let proto_output_name = output_names[1].clone(); // "output1"

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
            box_output_name,
            proto_output_name,
            input_height,
            input_width,
            confidence_threshold: 0.4,
            nms_threshold: 0.4,
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

        // --- Extract Raw Outputs ---
        let box_output = outputs
            .get(self.box_output_name.as_str())
            .ok_or_else(|| anyhow!("Box output not found"))?;
        let proto_output = outputs
            .get(self.proto_output_name.as_str())
            .ok_or_else(|| anyhow!("Prototype output not found"))?;

        // --- 1. Extract raw shapes and flat float slices natively from ORT ---
        let (box_shape, box_slice) = box_output.try_extract_tensor::<f32>()?;
        let (proto_shape, proto_slice) = proto_output.try_extract_tensor::<f32>()?;

        // --- 2. RESHAPE AND PERMUTE BOX_VIEW STRIDES TO MATCH EXPECTATIONS ---
        // Extract the raw dimensions as standard clean usize slices for ndarray mapping
        let box_dims: Vec<usize> = box_shape.iter().map(|&x| x as usize).collect();
        let proto_dims: Vec<usize> = proto_shape.iter().map(|&x| x as usize).collect();

        // Interpret the flat buffer using the raw output structure shape via ndarray
        let raw_box_array = ndarray::ArrayView3::from_shape((1, 116, 8400), box_slice)?;

        // .permuted_axes() swaps the channel and anchor indices cleanly in memory!
        let transposed_box_matrix = raw_box_array.permuted_axes([0, 2, 1]);

        // FIX: Manually copy the elements into a fresh, flat, contiguous Vec.
        // This physically clones the bytes sequentially, making it perfectly contiguous!
        let box_flat_slice: Vec<f32> = transposed_box_matrix.iter().copied().collect();

        // Reconstruct the proto_view safely using a standard dynamic dimensionality mapping conversion
        let proto_view = ndarray::ArrayViewD::from_shape(proto_dims.as_slice(), proto_slice)?;
        let proto_flat_slice = proto_view
            .as_slice()
            .ok_or_else(|| anyhow!("Proto view not contiguous"))?;

        // Explicit fixed shapes used by the native edgefirst tensor parser loops
        let edge_box_shape = vec![1, 8400, 116];

        // --- 3. Initialize Math Post-Processor Framework ---
        let config_outputs = edgefirst_decoder::config::ConfigOutputs {
            decoder_version: Some(edgefirst_decoder::configs::DecoderVersion::Yolov8),
            nms: Some(edgefirst_decoder::configs::Nms::Auto),
            outputs: vec![
                edgefirst_decoder::config::ConfigOutput::Detection(
                    edgefirst_decoder::configs::Detection {
                        decoder: edgefirst_decoder::configs::DecoderType::Ultralytics,
                        shape: edge_box_shape.clone(),
                        ..Default::default()
                    },
                ),
                edgefirst_decoder::config::ConfigOutput::Protos(
                    edgefirst_decoder::configs::Protos {
                        decoder: edgefirst_decoder::configs::DecoderType::Ultralytics,
                        shape: proto_dims.clone(),
                        ..Default::default()
                    },
                ),
            ],
        };

        let decoder = edgefirst_decoder::DecoderBuilder::new()
            .with_score_threshold(self.confidence_threshold)
            .with_iou_threshold(self.nms_threshold)
            .with_config(config_outputs)
            .build()
            .map_err(|e| anyhow!("Decoder build failure: {:?}", e))?;

        let mut detections: Vec<DetectBox> = Vec::with_capacity(50);
        let mut masks: Vec<Segmentation> = Vec::with_capacity(50);

        // --- 4. CONVERT TO NATIVE TENSORS VIA EXPOSED TRAIT ---
        // Pass the reference to our owned flat vector directly!
        let box_dyn = f32::slice_into_tensor_dyn(&box_flat_slice, &edge_box_shape)
            .map_err(|e| anyhow!("Failed to construct box TensorDyn: {:?}", e))?;
        let proto_dyn = f32::slice_into_tensor_dyn(proto_flat_slice, &proto_dims)
            .map_err(|e| anyhow!("Failed to construct proto TensorDyn: {:?}", e))?;

        // Run the synchronized decoder matrix pipeline safely by passing references to the TensorDyn containers
        decoder
            .decode(&[&box_dyn, &proto_dyn], &mut detections, &mut masks)
            .map_err(|e| anyhow!("Decoder matrix computation failure: {:?}", e))?;

        if detections.is_empty() {
            return Ok(());
        }

        // --- Apply Output Bitmask to Blackout Buffers ---
        let mut combined_mask = vec![0u8; orig_width * orig_height];

        for (det, mask) in detections.iter().zip(masks.iter()) {
            if det.label != 0 {
                // Target 'person' class category context matching v0.27.0 specs
                continue;
            }

            let raw_mask_grid = &mask.segmentation;
            let mask_shape = raw_mask_grid.shape();

            let mask_h = mask_shape[1];
            let mask_w = mask_shape[2];

            // Safely reference bounding box pixel borders from the public structural variables
            let x0 = (det.bbox.xmin as usize).min(orig_width);
            let y0 = (det.bbox.ymin as usize).min(orig_height);
            let x1 = (det.bbox.xmax as usize).min(orig_width);
            let y1 = (det.bbox.ymax as usize).min(orig_height);

            for y in y0..y1 {
                let norm_y = y as f32 / orig_height as f32;
                let src_y = ((norm_y * mask_h as f32) as usize).min(mask_h - 1);
                let row_offset = y * orig_width;

                for x in x0..x1 {
                    let norm_x = x as f32 / orig_width as f32;
                    let src_x = ((norm_x * mask_w as f32) as usize).min(mask_w - 1);

                    // Index the 3D matrix using [0, y, x] layout format
                    if raw_mask_grid[[0, src_y, src_x]] > 0 {
                        combined_mask[row_offset + x] = 1;
                    }
                }
            }
        }

        // --- Perform Structural Mask Dilation ---
        if self.dilation_iterations > 0 {
            combined_mask = VideoModel::dilate_mask(
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

#[derive(Debug, Clone)]
struct Detection {
    bbox: BoundingBox,
    confidence: f32,
    class_id: usize,
    coeffs: [f32; 32],
}
