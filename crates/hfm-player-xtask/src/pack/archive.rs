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

use crate::pack::download::DIST_DIR;

/// Create a platform‑specific archive of the dist/ folder.
pub fn archive_dist() -> Result<()> {
    let os = std::env::consts::OS;
    let archive_name = match os {
        "windows" => "hfm-player-windows-x64.zip",
        "linux" => "hfm-player-linux-x86_64.tar.gz",
        "macos" => "hfm-player-macos-universal.tar.gz",
        _ => bail!("Unsupported OS for archiving: {}", os),
    };

    let dist_path = Path::new(DIST_DIR);
    if !dist_path.exists() {
        bail!("Distribution folder does not exist: {}", DIST_DIR);
    }

    match os {
        "windows" => create_zip(dist_path, archive_name)?,
        "linux" | "macos" => create_tar_gz(dist_path, archive_name)?,
        _ => unreachable!(),
    }

    println!("Archive created: {}", archive_name);
    Ok(())
}

/// Create a ZIP archive (Windows).
fn create_zip(src_dir: &Path, archive_name: &str) -> Result<()> {
    let file = File::create(archive_name)?;
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
            // Add directory entry (optional, but some extractors expect it)
            zip.add_directory(name.to_str().unwrap(), options)?;
        }
    }
    zip.finish()?;
    Ok(())
}

/// Create a TAR.GZ archive (Linux/macOS).
fn create_tar_gz(src_dir: &Path, archive_name: &str) -> Result<()> {
    let tar_gz = File::create(archive_name)?;
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
