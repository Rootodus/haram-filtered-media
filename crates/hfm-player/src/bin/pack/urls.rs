//! Centralized URL generation for GStreamer and OpenVINO.

use crate::download::get_pypi_wheel_url;
use anyhow::{Result, bail};
use std::env;

pub const GSTREAMER_VERSION: &str = "1.27.90";
pub const GSTREAMER_PLUGINS_VERSION: &str = "1.28.6";
pub const OPENVINO_VERSION: &str = "2025.4.1";

pub fn gstreamer_runtime_url() -> Result<String> {
    let os = env::consts::OS;
    match os {
        "windows" => get_pypi_wheel_url("gstreamer-runtime", GSTREAMER_VERSION, "win_amd64"),
        "macos" => get_pypi_wheel_url(
            "gstreamer-runtime",
            GSTREAMER_VERSION,
            "macosx_10_13_universal2",
        ),
        _ => bail!("GStreamer runtime not available for {}", os),
    }
}

pub fn gstreamer_plugins_url() -> Result<String> {
    let os = env::consts::OS;
    match os {
        "windows" => {
            get_pypi_wheel_url("gstreamer-plugins", GSTREAMER_PLUGINS_VERSION, "win_amd64")
        }
        "macos" => get_pypi_wheel_url(
            "gstreamer-plugins",
            GSTREAMER_PLUGINS_VERSION,
            "macosx_10_13_universal2",
        ),
        _ => bail!("GStreamer plugins not available for {}", os),
    }
}

pub fn openvino_url() -> Result<String> {
    let os = env::consts::OS;
    match os {
        "windows" => Ok(format!(
            "https://storage.openvinotoolkit.org/repositories/openvino/packages/{}/windows/openvino_toolkit_windows_{}.20426.82bbf0292c5_x86_64.zip",
            OPENVINO_VERSION, OPENVINO_VERSION
        )),
        "linux" => Ok(format!(
            "https://storage.openvinotoolkit.org/repositories/openvino/packages/{}/ubuntu22/openvino_toolkit_ubuntu22_{}.20426.82bbf0292c5_x86_64.tgz",
            OPENVINO_VERSION, OPENVINO_VERSION
        )),
        "macos" => Ok(format!(
            "https://storage.openvinotoolkit.org/repositories/openvino/packages/{}/macos/openvino_toolkit_macos_12_6_{}.20426.82bbf0292c5_x86_64.tgz",
            OPENVINO_VERSION, OPENVINO_VERSION
        )),
        _ => bail!("OpenVINO not available for {}", os),
    }
}

/// Check all URLs for the current platform.
pub fn check_urls() -> Result<()> {
    println!(
        "Checking URLs for current platform (OS: {})",
        env::consts::OS
    );

    let runtime_url = gstreamer_runtime_url()?;
    println!("  GStreamer runtime: {}", runtime_url);
    let plugins_url = gstreamer_plugins_url()?;
    println!("  GStreamer plugins: {}", plugins_url);
    let ov_url = openvino_url()?;
    println!("  OpenVINO: {}", ov_url);

    // Optionally, you could do a HEAD request to verify existence,
    // but we'll just print them for now.
    println!("URL check complete.");
    Ok(())
}
