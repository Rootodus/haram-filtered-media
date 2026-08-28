//! Packaging script for hfm-player.
//!
//! Usage: cargo run --bin pack [--clean] [--check-urls]

mod archive;
mod bundle;
mod download;
mod gstreamer;
mod models;
mod openvino;
mod urls;

use anyhow::Result;
use std::env;

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    let clean = args.iter().any(|a| a == "--clean");
    let check = args.iter().any(|a| a == "--check-urls");

    if clean {
        download::clean()?;
        return Ok(());
    }

    if check {
        urls::check_urls()?;
        return Ok(());
    }

    println!("Starting packaging process...");
    println!("OS: {}", env::consts::OS);

    download::prepare_cache()?;
    gstreamer::prepare_gstreamer()?;
    openvino::prepare_openvino()?;
    models::prepare_models()?;
    bundle::bundle_binary()?;
    archive::archive_dist()?;

    println!("Packaging complete! Artifacts in dist/");
    Ok(())
}
