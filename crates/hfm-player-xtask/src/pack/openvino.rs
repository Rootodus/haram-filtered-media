//! OpenVINO packaging logic.

use anyhow::{Result, anyhow, bail};
use std::env;
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

use crate::pack::download::{CACHE_DIR, LIB_DIR, download_file, ensure_dir, extract_tar_gz, extract_zip};
use crate::pack::urls::openvino_url;

pub fn prepare_openvino() -> Result<()> {
    let os = env::consts::OS;
    match os {
        "windows" => prepare_openvino_windows()?,
        "linux" => prepare_openvino_linux()?,
        "macos" => prepare_openvino_macos()?,
        _ => println!("Unsupported OS: {}", os),
    }
    Ok(())
}

fn prepare_openvino_windows() -> Result<()> {
    let cache_dir = Path::new(CACHE_DIR).join("openvino");
    let extracted_dir = cache_dir.join("extracted");
    let zip_path = cache_dir.join("openvino.zip");
    ensure_dir(&cache_dir)?;

    let url = openvino_url()?;
    download_file(&url, &zip_path)?;
    if !extracted_dir.exists() {
        extract_zip(&zip_path, &extracted_dir)?;
    }

    let lib_dir = Path::new(LIB_DIR);
    ensure_dir(lib_dir)?;

    // Find the directory containing openvino.dll.
    let dll_dir = WalkDir::new(&extracted_dir)
        .into_iter()
        .find_map(|e| {
            if let Ok(entry) = e {
                let path = entry.path();
                if path.is_file()
                    && path.file_name().and_then(|n| n.to_str()) == Some("openvino.dll")
                {
                    return Some(path.parent().unwrap().to_path_buf());
                }
            }
            None
        })
        .ok_or_else(|| anyhow!("Could not find openvino.dll in extracted archive"))?;

    // Copy all .dll files from that directory.
    for entry in fs::read_dir(&dll_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("dll") {
            let dest = lib_dir.join(path.file_name().unwrap());
            fs::copy(&path, &dest)?;
        }
    }

    println!("OpenVINO DLLs copied to {}", lib_dir.display());
    Ok(())
}

fn prepare_openvino_linux() -> Result<()> {
    let cache_dir = Path::new(CACHE_DIR).join("openvino");
    let extracted_dir = cache_dir.join("extracted");
    let tgz_path = cache_dir.join("openvino.tgz");
    ensure_dir(&cache_dir)?;

    let url = openvino_url()?;
    download_file(&url, &tgz_path)?;
    if !extracted_dir.exists() {
        extract_tar_gz(&tgz_path, &extracted_dir)?;
    }

    let lib_dir = Path::new(LIB_DIR);
    ensure_dir(lib_dir)?;

    let runtime_dir = WalkDir::new(&extracted_dir).into_iter().find_map(|e| {
        if let Ok(entry) = e {
            let path = entry.path();
            if path.is_dir()
                && path.ends_with("intel64")
                && path.parent().map(|p| p.ends_with("lib")).unwrap_or(false)
            {
                Some(path.to_path_buf())
            } else {
                None
            }
        } else {
            None
        }
    });

    if let Some(runtime) = runtime_dir {
        for entry in fs::read_dir(runtime)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("so") {
                let dest = lib_dir.join(path.file_name().unwrap());
                fs::copy(&path, &dest)?;
            }
        }
    } else {
        bail!("Could not find OpenVINO .so files in extracted archive");
    }

    println!("OpenVINO .so files copied to {}", lib_dir.display());
    Ok(())
}

fn prepare_openvino_macos() -> Result<()> {
    let cache_dir = Path::new(CACHE_DIR).join("openvino");
    let extracted_dir = cache_dir.join("extracted");
    let tgz_path = cache_dir.join("openvino.tgz");
    ensure_dir(&cache_dir)?;

    let url = openvino_url()?;
    download_file(&url, &tgz_path)?;
    if !extracted_dir.exists() {
        extract_tar_gz(&tgz_path, &extracted_dir)?;
    }

    let lib_dir = Path::new(LIB_DIR);
    ensure_dir(lib_dir)?;

    let runtime_dir = WalkDir::new(&extracted_dir).into_iter().find_map(|e| {
        if let Ok(entry) = e {
            let path = entry.path();
            if path.is_dir()
                && path.ends_with("lib")
                && path
                    .parent()
                    .map(|p| p.ends_with("runtime"))
                    .unwrap_or(false)
            {
                Some(path.to_path_buf())
            } else {
                None
            }
        } else {
            None
        }
    });

    if let Some(runtime) = runtime_dir {
        for entry in fs::read_dir(runtime)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("dylib") {
                let dest = lib_dir.join(path.file_name().unwrap());
                fs::copy(&path, &dest)?;
            }
        }
    } else {
        bail!("Could not find OpenVINO .dylib files in extracted archive");
    }

    println!("OpenVINO .dylib files copied to {}", lib_dir.display());
    Ok(())
}
