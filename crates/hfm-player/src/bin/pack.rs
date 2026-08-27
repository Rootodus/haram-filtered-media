//! Packaging script for hfm-player.
//! Downloads and extracts GStreamer and OpenVINO runtimes,
//! then bundles them with the release binary.
//!
//! Usage: cargo run --bin pack [--clean]

use anyhow::{Context, Result, bail};
use flate2::read::GzDecoder;
use reqwest::blocking::Client;
use std::env;
use std::fs::{self, File};
use std::io::copy;
use std::path::{Path, PathBuf};
use tar::Archive;
use walkdir::WalkDir;
use zip::ZipArchive;

// ----------------------------------------------------------------------------
// Constants
// ----------------------------------------------------------------------------

const GSTREAMER_VERSION: &str = "1.28.6";
const OPENVINO_VERSION: &str = "2025.4.1";
const CACHE_DIR: &str = "target/cache";
const DIST_DIR: &str = "dist";
const LIB_DIR: &str = "dist/lib";

// Platform-specific URL patterns.
fn gstreamer_url() -> String {
    let os = env::consts::OS;
    match os {
        "windows" => format!(
            "https://gstreamer.freedesktop.org/data/pkg/windows/{}/msvc/gstreamer-1.0-msvc-x86_64-{}.msi",
            GSTREAMER_VERSION, GSTREAMER_VERSION
        ),
        "macos" => format!(
            "https://gstreamer.freedesktop.org/data/pkg/osx/{}/gstreamer-1.0-{}-universal.pkg",
            GSTREAMER_VERSION, GSTREAMER_VERSION
        ),
        _ => String::new(),
    }
}

fn openvino_url() -> String {
    let os = env::consts::OS;
    match os {
        "windows" => format!(
            "https://storage.openvinotoolkit.org/repositories/openvino/packages/{}/windows/openvino_toolkit_windows_{}.20426.82bbf0292c5_x86_64.zip",
            OPENVINO_VERSION, OPENVINO_VERSION
        ),
        "linux" => format!(
            "https://storage.openvinotoolkit.org/repositories/openvino/packages/{}/ubuntu22/openvino_toolkit_ubuntu22_{}.20426.82bbf0292c5_x86_64.tgz",
            OPENVINO_VERSION, OPENVINO_VERSION
        ),
        "macos" => format!(
            "https://storage.openvinotoolkit.org/repositories/openvino/packages/{}/macos/openvino_toolkit_macos_12_6_{}.20426.82bbf0292c5_x86_64.tgz",
            OPENVINO_VERSION, OPENVINO_VERSION
        ),
        _ => String::new(),
    }
}

fn openvino_archive_name() -> &'static str {
    match env::consts::OS {
        "windows" => "openvino.zip",
        "linux" => "openvino.tgz",
        "macos" => "openvino.tgz",
        _ => "",
    }
}

// ----------------------------------------------------------------------------
// Helpers
// ----------------------------------------------------------------------------

fn ensure_dir(path: &Path) -> Result<()> {
    if !path.exists() {
        fs::create_dir_all(path).with_context(|| format!("Failed to create {:?}", path))?;
    }
    Ok(())
}

fn download_file(url: &str, dest: &Path) -> Result<()> {
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

fn extract_zip(zip_path: &Path, dest_dir: &Path) -> Result<()> {
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

fn extract_tar_gz(tgz_path: &Path, dest_dir: &Path) -> Result<()> {
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

fn copy_files(src_pattern: &str, dest_dir: &Path, recursive: bool) -> Result<()> {
    // Simple glob: we assume src_pattern is a directory or a wildcard pattern with `*`.
    // We'll use walkdir to find files.
    let src_dir = Path::new(src_pattern);
    if src_dir.is_dir() {
        // Copy everything inside the directory.
        for entry in WalkDir::new(src_dir) {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                let rel = path.strip_prefix(src_dir)?;
                let dest_path = dest_dir.join(rel);
                if let Some(parent) = dest_path.parent() {
                    ensure_dir(parent)?;
                }
                fs::copy(path, &dest_path)?;
            }
        }
    } else {
        // Pattern like "*/bin/*.dll" – we need to walk the parent.
        // For simplicity, we'll handle the specific case: we know the extracted structure.
        // We'll copy from known directories.
        // For Windows GStreamer: extracted/*/bin/*.dll
        // We'll use walkdir on the parent.
        let parent = src_dir.parent().unwrap_or(Path::new(""));
        if parent.exists() {
            for entry in WalkDir::new(parent) {
                let entry = entry?;
                let path = entry.path();
                if path.is_file() {
                    // Check if path matches the pattern: contains "/bin/" and ends with ".dll"
                    if let Some(path_str) = path.to_str() {
                        if path_str.contains("/bin/") && path_str.ends_with(".dll") {
                            let dest_path = dest_dir.join(path.file_name().unwrap());
                            fs::copy(path, &dest_path)?;
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

// ----------------------------------------------------------------------------
// Main tasks
// ----------------------------------------------------------------------------

fn clean() -> Result<()> {
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

fn prepare_cache() -> Result<()> {
    let cache = Path::new(CACHE_DIR);
    ensure_dir(&cache.join("gstreamer"))?;
    ensure_dir(&cache.join("openvino"))?;
    Ok(())
}

fn prepare_gstreamer() -> Result<()> {
    let os = env::consts::OS;
    match os {
        "windows" => prepare_gstreamer_windows()?,
        "macos" => prepare_gstreamer_macos()?,
        "linux" => {
            println!("Linux: GStreamer is not bundled; system installation required.");
        }
        _ => println!("Unsupported OS: {}", os),
    }
    Ok(())
}

fn prepare_gstreamer_windows() -> Result<()> {
    let cache_dir = Path::new(CACHE_DIR).join("gstreamer");
    let extracted_dir = cache_dir.join("extracted");
    let msi_path = cache_dir.join(format!("gstreamer-{}-msvc-x86_64.msi", GSTREAMER_VERSION));
    ensure_dir(&cache_dir)?;

    // Download MSI if missing.
    if !msi_path.exists() {
        let url = gstreamer_url();
        download_file(&url, &msi_path)?;
    }

    // Extract using msiexec.
    if !extracted_dir.exists() {
        println!("Extracting GStreamer MSI...");
        let status = std::process::Command::new("msiexec")
            .args(&[
                "/a",
                msi_path.to_str().unwrap(),
                "/qb",
                &format!(
                    "TARGETDIR={}",
                    std::env::current_dir()?.join(&extracted_dir).display()
                ),
            ])
            .status()?;
        if !status.success() {
            bail!("msiexec extraction failed");
        }
        // After extraction, the structure is: extracted_dir/gstreamer/1.0/msvc_x86_64/
    }

    // Copy DLLs and plugins to dist/lib.
    let lib_dir = Path::new(LIB_DIR);
    ensure_dir(lib_dir)?;

    let base = extracted_dir
        .join("gstreamer")
        .join("1.0")
        .join("msvc_x86_64");
    if !base.exists() {
        bail!(
            "Extracted GStreamer not found at expected path: {}",
            base.display()
        );
    }

    // Copy DLLs from bin/
    let bin_dir = base.join("bin");
    if bin_dir.exists() {
        for entry in fs::read_dir(bin_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("dll") {
                let dest = lib_dir.join(path.file_name().unwrap());
                fs::copy(&path, &dest)?;
            }
        }
    }

    // Copy plugin directory.
    let plugins_src = base.join("lib").join("gstreamer-1.0");
    if plugins_src.exists() {
        let plugins_dest = lib_dir.join("gstreamer-1.0");
        ensure_dir(&plugins_dest)?;
        for entry in WalkDir::new(&plugins_src) {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                let rel = path.strip_prefix(&plugins_src)?;
                let dest = plugins_dest.join(rel);
                if let Some(parent) = dest.parent() {
                    ensure_dir(parent)?;
                }
                fs::copy(path, &dest)?;
            }
        }
    }

    println!("GStreamer DLLs and plugins copied to {}", lib_dir.display());
    Ok(())
}

fn prepare_gstreamer_macos() -> Result<()> {
    let cache_dir = Path::new(CACHE_DIR).join("gstreamer");
    let extracted_dir = cache_dir.join("extracted");
    let pkg_path = cache_dir.join(format!("gstreamer-1.0-{}-universal.pkg", GSTREAMER_VERSION));
    ensure_dir(&cache_dir)?;

    // Download if missing.
    if !pkg_path.exists() {
        let url = gstreamer_url();
        download_file(&url, &pkg_path)?;
    }

    // Expand the pkg.
    if !extracted_dir.exists() {
        println!("Expanding GStreamer pkg...");
        let status = std::process::Command::new("pkgutil")
            .args(&[
                "--expand",
                pkg_path.to_str().unwrap(),
                extracted_dir.to_str().unwrap(),
            ])
            .status()?;
        if !status.success() {
            bail!("pkgutil expand failed");
        }

        // The payload is usually inside a nested pkg. We'll find the actual payload file.
        // For simplicity, we'll attempt to extract the Payload file from the expanded structure.
        // Many GStreamer packages have a structure like:
        // extracted_dir/gstreamer-1.0-1.28.6-universal.pkg/Payload
        // We'll find the Payload file and extract it.
        let payload_path = extracted_dir.join("Payload");
        if payload_path.exists() {
            let payload_dir = extracted_dir.join("payload_extracted");
            ensure_dir(&payload_dir)?;
            let status = std::process::Command::new("sh")
                .arg("-c")
                .arg(&format!(
                    "cat '{}' | gunzip -dc | cpio -i",
                    payload_path.display()
                ))
                .current_dir(&payload_dir)
                .status()?;
            if !status.success() {
                bail!("Failed to extract Payload using cpio");
            }
            // Now the files are in payload_dir/Library/Frameworks/GStreamer.framework/
            // We'll copy from there.
        } else {
            // Alternative: use xar to extract.
            println!("Payload not found; trying xar...");
            // Not implemented fully; we'll assume the pkgutil expansion works.
        }
    }

    // After extraction, the framework is at: extracted_dir/payload_extracted/Library/Frameworks/GStreamer.framework/
    let framework_root = extracted_dir
        .join("payload_extracted")
        .join("Library")
        .join("Frameworks")
        .join("GStreamer.framework");
    if !framework_root.exists() {
        bail!(
            "GStreamer framework not found at {}",
            framework_root.display()
        );
    }

    let lib_dir = Path::new(LIB_DIR);
    ensure_dir(lib_dir)?;

    // Copy .dylib files from Versions/1.0/lib/
    let lib_src = framework_root.join("Versions").join("1.0").join("lib");
    if lib_src.exists() {
        for entry in fs::read_dir(lib_src)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("dylib") {
                let dest = lib_dir.join(path.file_name().unwrap());
                fs::copy(&path, &dest)?;
            }
        }
    }

    // Copy plugins if present (may be in lib/gstreamer-1.0)
    let plugins_src = lib_src.join("gstreamer-1.0");
    if plugins_src.exists() {
        let plugins_dest = lib_dir.join("gstreamer-1.0");
        ensure_dir(&plugins_dest)?;
        for entry in WalkDir::new(&plugins_src) {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                let rel = path.strip_prefix(&plugins_src)?;
                let dest = plugins_dest.join(rel);
                if let Some(parent) = dest.parent() {
                    ensure_dir(parent)?;
                }
                fs::copy(path, &dest)?;
            }
        }
    }

    println!(
        "GStreamer libraries and plugins copied to {}",
        lib_dir.display()
    );
    Ok(())
}

fn prepare_openvino() -> Result<()> {
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

    if !zip_path.exists() {
        let url = openvino_url();
        download_file(&url, &zip_path)?;
    }

    if !extracted_dir.exists() {
        extract_zip(&zip_path, &extracted_dir)?;
    }

    // Copy DLLs from runtime/bin/intel64/Release/
    let lib_dir = Path::new(LIB_DIR);
    ensure_dir(lib_dir)?;

    // The extracted structure may have a top-level folder.
    // We'll find the runtime directory.
    let runtime_dir = WalkDir::new(&extracted_dir)
        .into_iter()
        .find_entry(|e| {
            let path = e.path();
            path.is_dir()
                && path.ends_with("Release")
                && path.parent().map(|p| p.ends_with("bin")).unwrap_or(false)
        })
        .map(|e| e.path().to_path_buf());

    if let Some(runtime) = runtime_dir {
        for entry in fs::read_dir(runtime)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("dll") {
                let dest = lib_dir.join(path.file_name().unwrap());
                fs::copy(&path, &dest)?;
            }
        }
    } else {
        bail!("Could not find OpenVINO runtime DLLs in extracted archive");
    }

    println!("OpenVINO DLLs copied to {}", lib_dir.display());
    Ok(())
}

fn prepare_openvino_linux() -> Result<()> {
    let cache_dir = Path::new(CACHE_DIR).join("openvino");
    let extracted_dir = cache_dir.join("extracted");
    let tgz_path = cache_dir.join("openvino.tgz");
    ensure_dir(&cache_dir)?;

    if !tgz_path.exists() {
        let url = openvino_url();
        download_file(&url, &tgz_path)?;
    }

    if !extracted_dir.exists() {
        extract_tar_gz(&tgz_path, &extracted_dir)?;
    }

    // Copy .so files from runtime/lib/intel64/
    let lib_dir = Path::new(LIB_DIR);
    ensure_dir(lib_dir)?;

    let runtime_dir = WalkDir::new(&extracted_dir)
        .into_iter()
        .find_entry(|e| {
            let path = e.path();
            path.is_dir()
                && path.ends_with("intel64")
                && path.parent().map(|p| p.ends_with("lib")).unwrap_or(false)
        })
        .map(|e| e.path().to_path_buf());

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

    if !tgz_path.exists() {
        let url = openvino_url();
        download_file(&url, &tgz_path)?;
    }

    if !extracted_dir.exists() {
        extract_tar_gz(&tgz_path, &extracted_dir)?;
    }

    // Copy .dylib files from runtime/lib/
    let lib_dir = Path::new(LIB_DIR);
    ensure_dir(lib_dir)?;

    let runtime_dir = WalkDir::new(&extracted_dir)
        .into_iter()
        .find_entry(|e| {
            let path = e.path();
            path.is_dir()
                && path.ends_with("lib")
                && path
                    .parent()
                    .map(|p| p.ends_with("runtime"))
                    .unwrap_or(false)
        })
        .map(|e| e.path().to_path_buf());

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

fn bundle_binary() -> Result<()> {
    let os = env::consts::OS;
    let dist_dir = Path::new(DIST_DIR);
    let exe_name = if os == "windows" {
        "hfm-player.exe"
    } else {
        "hfm-player"
    };
    let src_bin = Path::new("../../target/release").join(exe_name);
    let dest_bin = dist_dir.join(exe_name);

    ensure_dir(dist_dir)?;

    if !src_bin.exists() {
        bail!("Release binary not found at {}", src_bin.display());
    }

    fs::copy(&src_bin, &dest_bin)?;
    println!("Copied binary to {}", dest_bin.display());

    // Set rpath on Linux/macOS.
    if os == "linux" {
        let status = std::process::Command::new("patchelf")
            .args(&["--set-rpath", "$ORIGIN/lib", dest_bin.to_str().unwrap()])
            .status()?;
        if !status.success() {
            eprintln!("Warning: patchelf failed (is it installed?)");
        }
    } else if os == "macos" {
        let status = std::process::Command::new("install_name_tool")
            .args(&[
                "-add_rpath",
                "@executable_path/lib",
                dest_bin.to_str().unwrap(),
            ])
            .status()?;
        if !status.success() {
            eprintln!("Warning: install_name_tool failed");
        }
    }

    Ok(())
}

// ----------------------------------------------------------------------------
// Main
// ----------------------------------------------------------------------------

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    let clean = args.iter().any(|a| a == "--clean");

    if clean {
        clean()?;
        return Ok(());
    }

    println!("Starting packaging process...");
    println!("OS: {}", env::consts::OS);

    prepare_cache()?;

    // GStreamer (only on Windows and macOS; Linux uses system).
    prepare_gstreamer()?;

    // OpenVINO (all platforms).
    prepare_openvino()?;

    // Copy dependencies (already done above).

    // Bundle binary.
    bundle_binary()?;

    println!("Packaging complete! Artifacts in {}", DIST_DIR);
    Ok(())
}
