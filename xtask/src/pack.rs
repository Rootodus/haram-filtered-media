//! Packaging logic for workspace crates.
//!
//! This module handles downloading dependencies, bundling binaries, and creating archives.
//! It is configured via [`CrateConfig`] to support multiple distributable crates.

mod archive;
mod bundle;
mod config;
mod download;
mod gstreamer;
mod models;
mod openvino;
mod urls;

pub use config::{PLAYER_FEATURES, CrateConfig, get_config};

use anyhow::Result;

/// Run the full packaging process for a given crate configuration.
pub fn run(config: &CrateConfig) -> Result<()> {
    println!("Starting packaging for {}", config.package);
    println!("OS: {}", std::env::consts::OS);

    download::prepare_cache()?;

    if config.needs_gstreamer {
        gstreamer::prepare_gstreamer(config)?;
    }
    if config.needs_openvino {
        openvino::prepare_openvino(config)?;
    }
    if config.needs_models {
        models::prepare_models(config)?;
    }

    bundle::bundle_binary(config)?;
    archive::archive_dist(config)?;

    println!("Packaging complete! Artifacts in {}", config.dist_dir().display());
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