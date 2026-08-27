//! Binary bundling and rpath setting.

use anyhow::{Result, bail};
use std::env;
use std::fs;
use std::path::Path;

use crate::download::{DIST_DIR, ensure_dir};

pub fn bundle_binary() -> Result<()> {
    let os = env::consts::OS;
    let dist_dir = Path::new(DIST_DIR);
    let exe_name = if os == "windows" {
        "hfm-player.exe"
    } else {
        "hfm-player"
    };
    let src_bin = Path::new("../../target/release").join(exe_name);
    let dest_bin = dist_dir.join(exe_name);

    ensure_dir(dist_dir)?;

    if !src_bin.exists() {
        bail!("Release binary not found at {}", src_bin.display());
    }

    fs::copy(&src_bin, &dest_bin)?;
    println!("Copied binary to {}", dest_bin.display());

    // Set rpath on Linux/macOS.
    if os == "linux" {
        let status = std::process::Command::new("patchelf")
            .args(&["--set-rpath", "$ORIGIN/lib", dest_bin.to_str().unwrap()])
            .status()?;
        if !status.success() {
            eprintln!("Warning: patchelf failed (is it installed?)");
        }
    } else if os == "macos" {
        let status = std::process::Command::new("install_name_tool")
            .args(&[
                "-add_rpath",
                "@executable_path/lib",
                dest_bin.to_str().unwrap(),
            ])
            .status()?;
        if !status.success() {
            eprintln!("Warning: install_name_tool failed");
        }
    }

    Ok(())
}
