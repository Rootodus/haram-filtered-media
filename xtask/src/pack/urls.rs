//! Centralized URL generation for GStreamer and OpenVINO.

use crate::pack::download::get_pypi_wheel_url;
use anyhow::{Result, bail};
use std::env;

// Both runtime and plugins now use the same stable 1.28.6 version.
pub const GSTREAMER_VERSION: &str = "1.28.6";
pub const GSTREAMER_PLUGINS_VERSION: &str = "1.28.6";
pub const OPENVINO_VERSION: &str = "2025.4.1";
pub const PPHUMANSEG_MODEL_URL: &str = "https://huggingface.co/opencv/human_segmentation_pphumanseg/resolve/main/human_segmentation_pphumanseg_2023mar.onnx?download=true";
pub const HTDEMUCS_MODEL_URL: &str = "https://huggingface.co/StemSplitio/htdemucs-ft-vocals-onnx/resolve/main/htdemucs_ft_vocals_fp16weights.onnx?download=true";

/// URL for the GStreamer core libraries (stable 1.28.6 via `gstreamer-libs`).
pub fn gstreamer_libs_url() -> Result<String> {
    let os = env::consts::OS;
    match os {
        "windows" => get_pypi_wheel_url("gstreamer-libs", GSTREAMER_VERSION, "win_amd64"),
        "macos" => get_pypi_wheel_url(
            "gstreamer-libs",
            GSTREAMER_VERSION,
            "macosx_10_13_universal2",
        ),
        _ => bail!("GStreamer libs not available for {}", os),
    }
}

/// URL for the GStreamer plugins (stable 1.28.6).
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

    let libs_url = gstreamer_libs_url()?;
    println!("  GStreamer libs: {}", libs_url);
    let plugins_url = gstreamer_plugins_url()?;
    println!("  GStreamer plugins: {}", plugins_url);
    let ov_url = openvino_url()?;
    println!("  OpenVINO: {}", ov_url);

    println!("URL check complete.");
    Ok(())
}
