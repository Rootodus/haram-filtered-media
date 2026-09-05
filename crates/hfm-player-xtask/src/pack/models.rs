//! ONNX model packaging logic.

use anyhow::Result;
use std::fs;
use std::path::Path;

use crate::pack::download::{download_file, ensure_dir};
use crate::pack::urls::{HTDEMUCS_MODEL_URL, PPHUMANSEG_MODEL_URL};

pub fn prepare_models() -> Result<()> {
    let cache_dir = Path::new("target/cache").join("models");
    ensure_dir(&cache_dir)?;

    let dist_models_dir = Path::new("dist").join("models");
    ensure_dir(&dist_models_dir)?;

    // --- PPHumanSeg model ---
    let pphumanseg_filename = "human_segmentation_pphumanseg_2023mar.onnx";
    let pphumanseg_cache = cache_dir.join(pphumanseg_filename);
    let pphumanseg_dest = dist_models_dir.join(pphumanseg_filename);

    if !pphumanseg_cache.exists() {
        println!("Downloading PPHumanSeg model...");
        download_file(PPHUMANSEG_MODEL_URL, &pphumanseg_cache)?;
    } else {
        println!("PPHumanSeg model already cached.");
    }

    if !pphumanseg_dest.exists() {
        fs::copy(&pphumanseg_cache, &pphumanseg_dest)?;
        println!("Copied PPHumanSeg model to dist/models/");
    }

    // --- HTDemucs model ---
    let htdemucs_filename = "htdemucs_ft_vocals_fp16weights.onnx";
    let htdemucs_cache = cache_dir.join(htdemucs_filename);
    let htdemucs_dest = dist_models_dir.join(htdemucs_filename);

    if !htdemucs_cache.exists() {
        println!("Downloading HTDemucs model...");
        download_file(HTDEMUCS_MODEL_URL, &htdemucs_cache)?;
    } else {
        println!("HTDemucs model already cached.");
    }

    if !htdemucs_dest.exists() {
        fs::copy(&htdemucs_cache, &htdemucs_dest)?;
        println!("Copied HTDemucs model to dist/models/");
    }

    Ok(())
}
