//! GStreamer packaging logic.

use anyhow::Result;
use std::env;
use std::path::Path;

use crate::download::{CACHE_DIR, LIB_DIR, copy_libraries, download_file, ensure_dir, extract_zip};
use crate::urls::{gstreamer_plugins_url, gstreamer_runtime_url};

pub fn prepare_gstreamer() -> Result<()> {
    let os = env::consts::OS;
    match os {
        "windows" => prepare_gstreamer_windows()?,
        "macos" => prepare_gstreamer_macos()?,
        "linux" => {
            println!("Linux: GStreamer is not bundled; system installation required.");
            println!("Please install GStreamer 1.0 via your package manager.");
        }
        _ => println!("Unsupported OS: {}", os),
    }
    Ok(())
}

fn prepare_gstreamer_windows() -> Result<()> {
    let cache_dir = Path::new(CACHE_DIR).join("gstreamer");
    ensure_dir(&cache_dir)?;

    let runtime_url = gstreamer_runtime_url()?;
    let runtime_whl = cache_dir.join("gstreamer_runtime.whl");
    download_file(&runtime_url, &runtime_whl)?;

    let plugins_url = gstreamer_plugins_url()?;
    let plugins_whl = cache_dir.join("gstreamer_plugins.whl");
    download_file(&plugins_url, &plugins_whl)?;

    let runtime_extract = cache_dir.join("runtime_extracted");
    extract_zip(&runtime_whl, &runtime_extract)?;

    let plugins_extract = cache_dir.join("plugins_extracted");
    extract_zip(&plugins_whl, &plugins_extract)?;

    let lib_dir = Path::new(LIB_DIR);
    ensure_dir(lib_dir)?;

    copy_libraries(&runtime_extract, lib_dir, Some("gstreamer-1.0"))?;
    copy_libraries(&plugins_extract, lib_dir, Some("gstreamer-1.0"))?;

    println!("GStreamer DLLs and plugins copied to {}", lib_dir.display());
    Ok(())
}

fn prepare_gstreamer_macos() -> Result<()> {
    let cache_dir = Path::new(CACHE_DIR).join("gstreamer");
    ensure_dir(&cache_dir)?;

    let runtime_url = gstreamer_runtime_url()?;
    let runtime_whl = cache_dir.join("gstreamer_runtime.whl");
    download_file(&runtime_url, &runtime_whl)?;

    let plugins_url = gstreamer_plugins_url()?;
    let plugins_whl = cache_dir.join("gstreamer_plugins.whl");
    download_file(&plugins_url, &plugins_whl)?;

    let runtime_extract = cache_dir.join("runtime_extracted");
    extract_zip(&runtime_whl, &runtime_extract)?;

    let plugins_extract = cache_dir.join("plugins_extracted");
    extract_zip(&plugins_whl, &plugins_extract)?;

    let lib_dir = Path::new(LIB_DIR);
    ensure_dir(lib_dir)?;

    copy_libraries(&runtime_extract, lib_dir, Some("gstreamer-1.0"))?;
    copy_libraries(&plugins_extract, lib_dir, Some("gstreamer-1.0"))?;

    println!(
        "GStreamer .dylib and plugins copied to {}",
        lib_dir.display()
    );
    Ok(())
}
