//! Binary bundling and rpath setting.

use anyhow::{Result, bail};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use super::config::CrateConfig;
use super::download::ensure_dir;

// Feature flags used for all builds of hfm-player.
const BUILD_FEATURES: &[&str] = &["only-gui-no-console", "no-default-video"];

pub fn bundle_binary(config: &CrateConfig) -> Result<()> {
    let os = env::consts::OS;
    let dist_dir = config.dist_dir();
    let lib_dir = config.lib_dir();
    ensure_dir(&lib_dir)?;

    // --- 1. Build the launcher ---
    println!("Building launcher...");
    let mut cmd = Command::new("cargo");
    cmd.args(["build", "--package", config.package, "--bin", config.launcher_bin, "--profile", "final-release"]);
    for feature in BUILD_FEATURES {
        cmd.args(["--features", feature]);
    }
    let status = cmd.status()?;
    if !status.success() {
        bail!("Failed to build launcher");
    }

    // --- 2. Build the core binary ---
    println!("Building core binary...");
    let mut cmd = Command::new("cargo");
    cmd.args(["build", "--package", config.package, "--bin", config.core_bin, "--profile", "final-release"]);
    for feature in BUILD_FEATURES {
        cmd.args(["--features", feature]);
    }
    let status = cmd.status()?;
    if !status.success() {
        bail!("Failed to build core binary");
    }

    // --- 3. Copy the core binary to lib/ with the renamed name ---
    let core_ext = if os == "windows" { ".exe" } else { "" };
    let src_core = workspace_root()
        .join("target/final-release")
        .join(format!("{}{}", config.core_bin, core_ext));
    let dest_core = lib_dir.join(format!("{}{}", config.core_renamed, core_ext));
    if !src_core.exists() {
        bail!("Core binary not found at {}", src_core.display());
    }
    fs::copy(&src_core, &dest_core)?;
    println!("Copied core binary to {}", dest_core.display());

    // --- 4. Copy the launcher to dist_dir/ as the main executable ---
    let launcher_ext = if os == "windows" { ".exe" } else { "" };
    let src_launcher = workspace_root()
        .join("target/final-release")
        .join(format!("{}{}", config.launcher_bin, launcher_ext));
    let dest_launcher = dist_dir.join(format!("{}{}", config.package, launcher_ext));
    if !src_launcher.exists() {
        bail!("Launcher binary not found at {}", src_launcher.display());
    }
    fs::copy(&src_launcher, &dest_launcher)?;
    println!("Copied launcher to {}", dest_launcher.display());

    // --- 5. Set rpath on Linux/macOS for the core binary ---
    if os == "linux" {
        let status = Command::new("patchelf")
            .args(&["--set-rpath", "$ORIGIN", dest_core.to_str().unwrap()])
            .status()?;
        if !status.success() {
            eprintln!("Warning: patchelf failed (is it installed?)");
        }
    } else if os == "macos" {
        let status = Command::new("install_name_tool")
            .args(&[
                "-add_rpath",
                "@executable_path",
                dest_core.to_str().unwrap(),
            ])
            .status()?;
        if !status.success() {
            eprintln!("Warning: install_name_tool failed");
        }
    }

    Ok(())
}

/// Returns the workspace root directory.
fn workspace_root() -> PathBuf {
    super::config::workspace_root()
}