//! Binary bundling and rpath setting.

use anyhow::{Result, bail};
use std::env;
use std::fs;
use std::path::Path;

use crate::download::{DIST_DIR, ensure_dir};

pub fn bundle_binary() -> Result<()> {
    let os = env::consts::OS;
    let dist_dir = Path::new(DIST_DIR);
    let lib_dir = dist_dir.join("lib");
    ensure_dir(&lib_dir)?;

    // --- 1. Build the launcher with final-release profile ---
    println!("Building launcher...");
    let status = std::process::Command::new("cargo")
        .args(["build", "--bin", "launcher", "--profile", "final-release"])
        .status()?;
    if !status.success() {
        bail!("Failed to build launcher");
    }

    // --- 2. Copy the core binary (built with final-release) to lib/ ---
    let exe_name = if os == "windows" {
        "hfm-player.exe"
    } else {
        "hfm-player"
    };
    let src_bin = Path::new("../../target/final-release").join(exe_name);
    let dest_bin = lib_dir.join(exe_name);
    if !src_bin.exists() {
        bail!("Release binary not found at {}", src_bin.display());
    }
    fs::copy(&src_bin, &dest_bin)?;
    println!("Copied core binary to {}", dest_bin.display());

    // --- 3. Copy the launcher to dist/ with a descriptive name ---
    let launcher_name = if os == "windows" {
        "haram-filtered-media-player.exe"
    } else {
        "haram-filtered-media-player"
    };
    let launcher_src = Path::new("../../target/final-release").join(if os == "windows" {
        "launcher.exe"
    } else {
        "launcher"
    });
    let launcher_dest = dist_dir.join(launcher_name);
    if !launcher_src.exists() {
        bail!("Launcher binary not found at {}", launcher_src.display());
    }
    fs::copy(&launcher_src, &launcher_dest)?;
    println!("Copied launcher to {}", launcher_dest.display());

    // --- 4. Set rpath on Linux/macOS ---
    if os == "linux" {
        let status = std::process::Command::new("patchelf")
            .args(&["--set-rpath", "$ORIGIN", dest_bin.to_str().unwrap()])
            .status()?;
        if !status.success() {
            eprintln!("Warning: patchelf failed (is it installed?)");
        }
    } else if os == "macos" {
        let status = std::process::Command::new("install_name_tool")
            .args(&["-add_rpath", "@executable_path", dest_bin.to_str().unwrap()])
            .status()?;
        if !status.success() {
            eprintln!("Warning: install_name_tool failed");
        }
    }

    Ok(())
}
