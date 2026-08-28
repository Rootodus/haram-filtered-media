//! Download and extraction utilities.

use anyhow::{Context, Result, bail};
use flate2::read::GzDecoder;
use reqwest::blocking::Client;
use serde_json::Value;
use std::fs::{self, File};
use std::io::copy;
use std::path::{Path, PathBuf};
use tar::Archive;
use walkdir::WalkDir;
use zip::ZipArchive;

pub const CACHE_DIR: &str = "target/cache";
pub const DIST_DIR: &str = "dist";
pub const LIB_DIR: &str = "dist/lib";

pub fn ensure_dir(path: &Path) -> Result<()> {
    if !path.exists() {
        fs::create_dir_all(path).with_context(|| format!("Failed to create {:?}", path))?;
    }
    Ok(())
}

pub fn download_file(url: &str, dest: &Path) -> Result<()> {
    if dest.exists() {
        println!("Already downloaded: {}", dest.display());
        return Ok(());
    }
    println!("Downloading {} ...", url);
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()?;
    let resp = client.get(url).send()?;
    if !resp.status().is_success() {
        bail!("Failed to download {}: {}", url, resp.status());
    }
    let mut file = File::create(dest)?;
    copy(&mut resp.bytes()?.as_ref(), &mut file)?;
    println!("Saved to {}", dest.display());
    Ok(())
}

pub fn extract_zip(zip_path: &Path, dest_dir: &Path) -> Result<()> {
    if dest_dir.exists() && dest_dir.read_dir()?.next().is_some() {
        println!("Already extracted: {}", dest_dir.display());
        return Ok(());
    }
    println!(
        "Extracting {} to {}",
        zip_path.display(),
        dest_dir.display()
    );
    let file = File::open(zip_path)?;
    let mut archive = ZipArchive::new(file)?;
    archive.extract(dest_dir)?;
    Ok(())
}

pub fn extract_tar_gz(tgz_path: &Path, dest_dir: &Path) -> Result<()> {
    if dest_dir.exists() && dest_dir.read_dir()?.next().is_some() {
        println!("Already extracted: {}", dest_dir.display());
        return Ok(());
    }
    println!(
        "Extracting {} to {}",
        tgz_path.display(),
        dest_dir.display()
    );
    let tar_gz = File::open(tgz_path)?;
    let tar = GzDecoder::new(tar_gz);
    let mut archive = Archive::new(tar);
    archive.unpack(dest_dir)?;
    Ok(())
}

pub fn get_pypi_wheel_url(package: &str, version: &str, platform_substr: &str) -> Result<String> {
    let url = format!("https://pypi.org/pypi/{}/{}/json", package, version);
    println!("Fetching PyPI metadata from {}", url);
    let client = Client::new();
    let resp = client.get(&url).send()?;
    if !resp.status().is_success() {
        bail!("Failed to fetch PyPI metadata: {}", resp.status());
    }
    let json: Value = resp.json()?;
    let urls = json["urls"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("No 'urls' array in PyPI response"))?;
    for entry in urls {
        if let Some(filename) = entry["filename"].as_str() {
            if filename.ends_with(".whl") && filename.contains(platform_substr) {
                if filename.contains("cp39-abi3") {
                    if let Some(url) = entry["url"].as_str() {
                        return Ok(url.to_string());
                    }
                }
            }
        }
    }
    bail!(
        "No matching wheel found for package={}, version={}, platform={}",
        package,
        version,
        platform_substr
    );
}

pub fn clean() -> Result<()> {
    let dist = Path::new(DIST_DIR);
    let cache = Path::new(CACHE_DIR);
    if dist.exists() {
        fs::remove_dir_all(dist)?;
        println!("Removed {}", DIST_DIR);
    }
    if cache.exists() {
        fs::remove_dir_all(cache)?;
        println!("Removed {}", CACHE_DIR);
    }
    Ok(())
}

pub fn prepare_cache() -> Result<()> {
    let cache = Path::new(CACHE_DIR);
    ensure_dir(&cache.join("gstreamer"))?;
    ensure_dir(&cache.join("openvino"))?;
    Ok(())
}

pub fn copy_libraries(src_dir: &Path, dest_dir: &Path, plugin_subdir: Option<&str>) -> Result<()> {
    ensure_dir(dest_dir)?;
    let ext = if cfg!(windows) {
        "dll"
    } else if cfg!(target_os = "macos") {
        "dylib"
    } else {
        "so"
    };
    // Walk the source and copy all files with the correct extension.
    for entry in WalkDir::new(src_dir) {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            if let Some(ext_os) = path.extension() {
                if ext_os == ext {
                    let dest = dest_dir.join(path.file_name().unwrap());
                    fs::copy(path, &dest)?;
                }
            }
        }
    }

    // If a plugin subdirectory is requested, find it and copy its contents.
    if let Some(sub) = plugin_subdir {
        // Walk the source tree to find the first directory named `sub`.
        if let Some(plugin_src) = find_plugin_dir(src_dir, sub) {
            let plugin_dest = dest_dir.join(sub);
            ensure_dir(&plugin_dest)?;
            for entry in WalkDir::new(&plugin_src) {
                let entry = entry?;
                let path = entry.path();
                if path.is_file() {
                    let rel = path.strip_prefix(&plugin_src)?;
                    let dest = plugin_dest.join(rel);
                    if let Some(parent) = dest.parent() {
                        ensure_dir(parent)?;
                    }
                    fs::copy(path, &dest)?;
                }
            }
        }
    }
    Ok(())
}

/// Recursively find a directory with the given name under `root`.
fn find_plugin_dir(root: &Path, name: &str) -> Option<PathBuf> {
    WalkDir::new(root).into_iter().find_map(|e| {
        if let Ok(entry) = e {
            let path = entry.path();
            if path.is_dir() && path.file_name().and_then(|n| n.to_str()) == Some(name) {
                return Some(path.to_path_buf());
            }
        }
        None
    })
}
