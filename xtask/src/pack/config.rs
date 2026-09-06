//! Configuration for packaging workspace crates.

use anyhow::{Result, bail};
use std::env;
use std::path::PathBuf;

/// Configuration for packaging a specific workspace crate.
#[derive(Debug, Clone)]
pub struct CrateConfig {
    /// The cargo package name (e.g., "hfm-player").
    pub package: &'static str,
    /// The name of the core engine binary (without extension). This is the binary that does the actual work.
    pub core_bin: &'static str,
    /// The name to rename the core binary to after copying to `lib/` (without extension).
    pub core_renamed: &'static str,
    /// The name of the launcher binary (without extension). This is the thin wrapper that starts the core.
    pub launcher_bin: &'static str,
    /// The subdirectory under `dist/` (e.g., "hfm-player").
    pub dist_subdir: &'static str,
    /// Prefix for the final archive (e.g., "hfm-player").
    pub archive_prefix: &'static str,
    /// Whether this crate needs GStreamer bundled.
    pub needs_gstreamer: bool,
    /// Whether this crate needs OpenVINO bundled.
    pub needs_openvino: bool,
    /// Whether this crate needs the ONNX models.
    pub needs_models: bool,
}

impl CrateConfig {
    /// Returns the absolute path to this crate's distribution folder.
    pub fn dist_dir(&self) -> PathBuf {
        workspace_root().join("dist").join(self.dist_subdir)
    }

    /// Returns the absolute path to this crate's `lib/` folder.
    pub fn lib_dir(&self) -> PathBuf {
        self.dist_dir().join("lib")
    }

    /// Returns the archive name for the current OS.
    pub fn archive_name(&self) -> String {
        let os = env::consts::OS;
        match os {
            "windows" => format!("{}-windows-x64.zip", self.archive_prefix),
            "linux" => format!("{}-linux-x86_64.tar.gz", self.archive_prefix),
            "macos" => format!("{}-macos-universal.tar.gz", self.archive_prefix),
            _ => panic!("Unsupported OS: {}", os),
        }
    }
}

/// Registry of all known distributable crates.
pub fn get_config(name: &str) -> Result<&'static CrateConfig> {
    match name {
        "hfm-player" => Ok(&CrateConfig {
            package: "hfm-player",
            core_bin: "hfm-player",
            core_renamed: "hfm-player-core",
            launcher_bin: "launcher",
            dist_subdir: "hfm-player",
            archive_prefix: "hfm-player",
            needs_gstreamer: true,
            needs_openvino: true,
            needs_models: true,
        }),
        // Add future crates here, e.g.:
        // "hfm-cli" => Ok(&CrateConfig {
        //     package: "hfm-cli",
        //     core_bin: "hfm-cli",
        //     core_renamed: "hfm-cli",
        //     launcher_bin: "hfm-cli",  // no separate launcher
        //     dist_subdir: "hfm-cli",
        //     archive_prefix: "hfm-cli",
        //     needs_gstreamer: false,
        //     needs_openvino: false,
        //     needs_models: false,
        // }),
        _ => bail!("Unknown crate for packaging: {}", name),
    }
}

/// Returns the workspace root directory.
/// 
/// Since this crate is at `workspace_root/xtask/`, we need to go up one level
/// from `CARGO_MANIFEST_DIR` to get the actual workspace root.
pub fn workspace_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // Go up one level: from /xtask/ to / (the workspace root)
    manifest_dir.parent().unwrap().to_path_buf()
}
