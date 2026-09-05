//! Automation tasks for hfm-player.
//!
//! Usage: cargo xtask [TASK]
//!
//! Tasks:
//!   clean         Remove dist/ and target/cache/
//!   build         Build the player (final-release profile)
//!   pack          Run the pack logic (download deps, bundle)
//!   dist          Build + pack (default)
//!   check-urls    Validate download URLs

mod pack;

use std::env;
use std::process::Command;

fn main() {
    let args: Vec<String> = env::args().collect();
    let task = args.get(1).map(String::as_str).unwrap_or("dist");

    match task {
        "clean" => clean(),
        "build" => build(),
        "pack" => pack(),
        "dist" => dist(),
        "check-urls" => check_urls(),
        _ => {
            eprintln!("Unknown task: {}", task);
            eprintln!("Available: clean, build, pack, dist, check-urls");
            std::process::exit(1);
        }
    }
}

fn clean() {
    if let Err(e) = pack::clean() {
        eprintln!("Clean failed: {}", e);
        std::process::exit(1);
    }
}

fn check_urls() {
    if let Err(e) = pack::check_urls() {
        eprintln!("URL check failed: {}", e);
        std::process::exit(1);
    }
}

fn build() {
    run_cargo("build", &[
        "--package", "hfm-player",
        "--profile", "final-release",
        "--features", "only-gui-no-console,no-default-video",
    ]);
}

fn pack() {
    if let Err(e) = pack::run() {
        eprintln!("Pack failed: {}", e);
        std::process::exit(1);
    }
}

fn dist() {
    build();
    pack();
}

fn run_cargo(command: &str, args: &[&str]) {
    let status = Command::new("cargo")
        .arg(command)
        .args(args)
        .status()
        .expect("Failed to run cargo");
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
}