//! Archive creation for distribution.

use anyhow::{Result, bail};
use flate2::Compression;
use flate2::write::GzEncoder;
use std::env;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use tar::Builder;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

use crate::pack::download::dist_dir;

/// Returns the workspace root directory.
fn workspace_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.parent().unwrap().parent().unwrap().to_path_buf()
}

/// Create a platform‑specific archive of the dist/ folder at the workspace root.
pub fn archive_dist() -> Result<()> {
    let os = std::env::consts::OS;
    let archive_name = match os {
        "windows" => "hfm-player-windows-x64.zip",
        "linux" => "hfm-player-linux-x86_64.tar.gz",
        "macos" => "hfm-player-macos-universal.tar.gz",
        _ => bail!("Unsupported OS for archiving: {}", os),
    };

    let dist_path = dist_dir();
    if !dist_path.exists() {
        bail!("Distribution folder does not exist: {}", dist_path.display());
    }

    let archive_path = workspace_root().join(archive_name);

    match os {
        "windows" => create_zip(&dist_path, &archive_path)?,
        "linux" | "macos" => create_tar_gz(&dist_path, &archive_path)?,
        _ => unreachable!(),
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
