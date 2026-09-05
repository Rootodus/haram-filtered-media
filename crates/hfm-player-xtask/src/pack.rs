//! Packaging logic for hfm-player.
//!
//! This module handles downloading dependencies, bundling, and creating archives.

mod archive;
mod bundle;
mod download;
mod gstreamer;
mod models;
mod openvino;
mod urls;

use anyhow::Result;

/// Run the full packaging process.
pub fn run() -> Result<()> {
    println!("Starting packaging process...");
    println!("OS: {}", std::env::consts::OS);

    download::prepare_cache()?;
    gstreamer::prepare_gstreamer()?;
    openvino::prepare_openvino()?;
    models::prepare_models()?;
    bundle::bundle_binary()?;
    archive::archive_dist()?;

    println!("Packaging complete! Artifacts in dist/");
    Ok(())
}

/// Remove distribution and cache directories.
pub fn clean() -> Result<()> {
    download::clean()
}

/// Validate all download URLs.
pub fn check_urls() -> Result<()> {
    urls::check_urls()
}
