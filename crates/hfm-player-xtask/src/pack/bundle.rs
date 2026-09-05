//! Binary bundling and rpath setting.

use anyhow::{Result, bail};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::pack::download::{DIST_DIR, ensure_dir};

/// Returns the workspace root directory.
fn workspace_root() -> PathBuf {
    // CARGO_MANIFEST_DIR points to <workspace>/crates/hfm-player-xtask/
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // Go up two levels to the workspace root.
    manifest_dir.parent().unwrap().parent().unwrap().to_path_buf()
}

pub fn bundle_binary() -> Result<()> {
    let os = env::consts::OS;
    let dist_dir = Path::new(DIST_DIR);
    let lib_dir = dist_dir.join("lib");
    ensure_dir(&lib_dir)?;

    // --- 1. Build the launcher ---
    println!("Building launcher...");
    let status = std::process::Command::new("cargo")
        .args(["build", "--bin", "launcher", "--profile", "final-release"])
        .status()?;
    if !status.success() {
        bail!("Failed to build launcher");
    }

    // --- 2. Copy the core binary to lib/ as hfm-player-core ---
    let core_name = if os == "windows" {
        "hfm-player-core.exe"
    } else {
        "hfm-player-core"
    };
    let workspace_root = workspace_root();
    let src_core = workspace_root
        .join("target/final-release")
        .join(if os == "windows" { "hfm-player.exe" } else { "hfm-player" });
    let dest_core = lib_dir.join(core_name);
    if !src_core.exists() {
        bail!("Release binary not found at {}", src_core.display());
    }
    fs::copy(&src_core, &dest_core)?;
    println!("Copied core binary to {}", dest_core.display());

    // --- 3. Copy the launcher to dist/ as hfm-player ---
    let launcher_name = if os == "windows" {
        "hfm-player.exe"
    } else {
        "hfm-player"
    };
    let src_launcher = workspace_root
        .join("target/final-release")
        .join(if os == "windows" { "launcher.exe" } else { "launcher" });
    let dest_launcher = dist_dir.join(launcher_name);
    if !src_launcher.exists() {
        bail!("Launcher binary not found at {}", src_launcher.display());
    }
    fs::copy(&src_launcher, &dest_launcher)?;
    println!("Copied launcher to {}", dest_launcher.display());

    // --- 4. Set rpath on Linux/macOS for the core binary ---
    if os == "linux" {
        let status = std::process::Command::new("patchelf")
            .args(&["--set-rpath", "$ORIGIN", dest_core.to_str().unwrap()])
            .status()?;
        if !status.success() {
            eprintln!("Warning: patchelf failed (is it installed?)");
        }
    } else if os == "macos" {
        let status = std::process::Command::new("install_name_tool")
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
