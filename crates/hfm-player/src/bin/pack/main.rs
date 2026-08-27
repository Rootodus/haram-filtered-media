//! Packaging script for hfm-player.
//!
//! Usage: cargo run --bin pack [--clean] [--check-urls]

mod bundle;
mod download;
mod gstreamer;
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
    bundle::bundle_binary()?;

    println!("Packaging complete! Artifacts in dist/");
    Ok(())
}
