//! Archive creation for distribution.

use anyhow::{Result, bail};
use flate2::Compression;
use flate2::write::GzEncoder;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use tar::Builder;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

use super::config::CrateConfig;
use super::config::workspace_root;

/// Create a platform‑specific archive of the crate's dist folder.
pub fn archive_dist(config: &CrateConfig) -> Result<()> {
    let dist_path = config.dist_dir();
    if !dist_path.exists() {
        bail!("Distribution folder does not exist: {}", dist_path.display());
    }

    let archive_name = config.archive_name();
    let archive_path = workspace_root().join(archive_name);

    let os = std::env::consts::OS;
    match os {
        "windows" => create_zip(&dist_path, &archive_path)?,
        "linux" | "macos" => create_tar_gz(&dist_path, &archive_path)?,
        _ => bail!("Unsupported OS for archiving: {}", os),
    }

    println!("Archive created: {}", archive_path.display());
    Ok(())
}

/// Create a ZIP archive (Windows).
fn create_zip(src_dir: &Path, archive_path: &Path) -> Result<()> {
    let file = File::create(archive_path)?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default();

    for entry in walkdir::WalkDir::new(src_dir) {
        let entry = entry?;
        let path = entry.path();
        let name = path.strip_prefix(src_dir)?;
        if path.is_file() {
            zip.start_file(name.to_str().unwrap(), options)?;
            let mut f = File::open(path)?;
            let mut buf = Vec::new();
            f.read_to_end(&mut buf)?;
            zip.write_all(&buf)?;
        } else {
            zip.add_directory(name.to_str().unwrap(), options)?;
        }
    }
    zip.finish()?;
    Ok(())
}

/// Create a TAR.GZ archive (Linux/macOS).
fn create_tar_gz(src_dir: &Path, archive_path: &Path) -> Result<()> {
    let tar_gz = File::create(archive_path)?;
    let encoder = GzEncoder::new(tar_gz, Compression::default());
    let mut tar = Builder::new(encoder);

    for entry in walkdir::WalkDir::new(src_dir) {
        let entry = entry?;
        let path = entry.path();
        let name = path.strip_prefix(src_dir)?;
        if path.is_file() {
            tar.append_file(name, &mut File::open(path)?)?;
        } else {
            tar.append_dir(name, path)?;
        }
    }
    tar.finish()?;
    Ok(())
}