use std::sync::OnceLock;

/// All debug toggles read from environment variables.
#[derive(Debug, Clone)]
pub struct DebugConfig {
    pub renderdoc_capture: bool,
    pub renderdoc_max_frames: u32,
    pub dump_frames_headless: bool, // formerly #[cfg(feature = "debug_captures")]
    pub inference_profiling: bool,  // to control ort session profiling
    pub gpu_validation: bool,
    pub log_level: LogLevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

// Global, lazily initialised config.
static CONFIG: OnceLock<DebugConfig> = OnceLock::new();

impl DebugConfig {
    /// Load config from environment variables. Call once in main().
    pub fn init() -> &'static DebugConfig {
        CONFIG.get_or_init(|| DebugConfig {
            renderdoc_capture: env_bool("RENDERDOC_CAPTURE", false),
            renderdoc_max_frames: env_parse("RENDERDOC_MAX_FRAMES", 1).max(1),
            dump_frames_headless: env_bool("DUMP_FRAMES_HEADLESS", false),
            inference_profiling: env_bool("PROFILE_INFERENCE", false),
            gpu_validation: env_bool("GPU_VALIDATION", false),
            log_level: match std::env::var("LOG_LEVEL")
                .unwrap_or_default()
                .to_lowercase()
                .as_str()
            {
                "trace" => LogLevel::Trace,
                "debug" => LogLevel::Debug,
                "info" => LogLevel::Info,
                "warn" => LogLevel::Warn,
                "error" => LogLevel::Error,
                _ => LogLevel::Info,
            },
        })
    }

    pub fn get() -> &'static DebugConfig {
        CONFIG
            .get()
            .expect("DebugConfig not initialised – call DebugConfig::init() first")
    }
}

fn env_bool(key: &str, default: bool) -> bool {
    std::env::var(key)
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(default)
}

fn env_parse<T: std::str::FromStr>(key: &str, default: T) -> T {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}
