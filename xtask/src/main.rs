//! Automation tasks for workspace crates.
//!
//! Usage: cargo xtask [TASK] [CRATE]
//!
//! Tasks:
//!   clean         Remove dist/ and target/cache/
//!   build         Build the player (final-release profile)
//!   pack          Run the pack logic (download deps, bundle)
//!   dist          Build + pack (default)
//!   check-urls    Validate download URLs
//!
//! CRATE defaults to "hfm-player" if not specified.

mod pack;

use std::env;
use std::process::Command;
use pack::{PLAYER_FEATURES, get_config};

fn main() {
    let args: Vec<String> = env::args().collect();
    let task = args.get(1).map(String::as_str).unwrap_or("dist");
    let crate_name = args.get(2).map(String::as_str).unwrap_or("hfm-player");

    match task {
        "clean" => clean(),
        "build" => build(),
        "pack" => pack(crate_name),
        "dist" => dist(crate_name),
        "check-urls" => check_urls(),
        _ => {
            eprintln!("Unknown task: {}", task);
            eprintln!("Available: clean, build, pack, dist, check-urls");
            eprintln!("Usage: cargo xtask [TASK] [CRATE]  (default CRATE = hfm-player)");
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
    let mut cmd = Command::new("cargo");
    cmd.args(["build", "--package", "hfm-player", "--profile", "final-release"]);
    for feature in PLAYER_FEATURES {
        cmd.args(["--features", feature]);
    }
    run_cargo_cmd(cmd);
}

fn run_cargo_cmd(mut cmd: Command) {
    let status = cmd.status().expect("Failed to run cargo");
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
}

fn pack(crate_name: &str) {
    let config = match get_config(crate_name) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    };
    if let Err(e) = pack::run(config) {
        eprintln!("Pack failed: {}", e);
        std::process::exit(1);
    }
}

fn dist(crate_name: &str) {
    build();
    pack(crate_name);
}
