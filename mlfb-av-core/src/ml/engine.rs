use anyhow::{Result, anyhow};
use ort::{session::Session, session::builder::GraphOptimizationLevel};

/// Handles cross-platform hardware acceleration backend assignment.
/// Safely cleans up internal C++ pointers to bypass Send/Sync compilation errors.
pub fn init_session(path: &str) -> Result<Session> {
    println!("Instantiating ONNX runtime execution providers...");

    // --- 1. APPLE SILICON TRACK ---
    #[cfg(target_vendor = "apple")]
    {
        use ort::ep::CoreMLExecutionProvider;
        let mut builder =
            Session::builder().map_err(|e| anyhow!("Failed to create session builder: {}", e))?;
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
                println!("SUCCESS: CoreML (Apple Silicon NPU/Metal) hardware backend is active.");
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

        // --- 1. Attempt OpenVINO first, with CPU fallback disabled ---
        let mut ov_builder =
            Session::builder().map_err(|e| anyhow!("Failed to create OpenVINO builder: {}", e))?;

        ov_builder = ov_builder
            .with_optimization_level(GraphOptimizationLevel::Level1)
            .map_err(|e| anyhow!("Failed to set OpenVINO optimization level: {:?}", e))?;

        ov_builder = ov_builder
            .with_intra_threads(1)
            .map_err(|e| anyhow!("Failed to set intra threads: {:?}", e))?;

        ov_builder = ov_builder
            .with_disable_cpu_fallback()
            .map_err(|e| anyhow!("Failed to disable CPU fallback for OpenVINO: {:?}", e))?;

        ov_builder = ov_builder
            .with_execution_providers([OpenVINOExecutionProvider::default()
                .with_device_type("GPU")
                .build()])
            .map_err(|e| anyhow!("Failed to set OpenVINO provider: {:?}", e))?;

        match ov_builder.commit_from_file(path) {
            Ok(s) => {
                println!(
                    "SUCCESS: Intel OpenVINO iGPU hardware backend is active (CPU fallback disabled)."
                );
                return Ok(s);
            }
            Err(e) => {
                println!(
                    "OpenVINO failed to initialize with CPU fallback disabled: {}\n\
                Falling back to DirectML...",
                    e
                );
            }
        }

        // --- 2. Fallback to DirectML (also with CPU fallback disabled) ---
        let mut dml_builder =
            Session::builder().map_err(|e| anyhow!("Failed to create DirectML builder: {}", e))?;

        dml_builder = dml_builder
            .with_optimization_level(GraphOptimizationLevel::Level1)
            .map_err(|e| anyhow!("Failed to set DirectML optimization level: {:?}", e))?;

        dml_builder = dml_builder
            .with_intra_threads(1)
            .map_err(|e| anyhow!("Failed to set intra threads: {:?}", e))?;

        dml_builder = dml_builder
            .with_disable_cpu_fallback()
            .map_err(|e| anyhow!("Failed to disable CPU fallback for DirectML: {:?}", e))?;

        dml_builder = dml_builder
            .with_execution_providers([DirectMLExecutionProvider::default().build()])
            .map_err(|e| anyhow!("Failed to set DirectML provider: {:?}", e))?;

        match dml_builder.commit_from_file(path) {
            Ok(s) => {
                println!("SUCCESS: DirectML hardware backend is active (CPU fallback disabled).");
                return Ok(s);
            }
            Err(e) => {
                println!(
                    "DirectML also failed with CPU fallback disabled: {}\n\
                No viable GPU execution provider.",
                    e
                );
                return Err(anyhow!(
                    "No GPU execution provider without CPU fallback. \
                The model may contain unsupported operators for both OpenVINO and DirectML."
                ));
            }
        }
    }

    // --- 3. LINUX TRACK ---
    #[cfg(target_os = "linux")]
    {
        use ort::ep::OpenVINOExecutionProvider;
        let mut ov_builder =
            Session::builder().map_err(|e| anyhow!("Failed to create session builder: {}", e))?;
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
    let mut cpu_builder =
        Session::builder().map_err(|e| anyhow!("Failed to create CPU fallback builder: {}", e))?;
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
