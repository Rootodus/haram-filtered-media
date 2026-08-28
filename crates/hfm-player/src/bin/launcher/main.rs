//! Launcher that sets PATH and spawns the core binary from lib/.

#![cfg_attr(windows, windows_subsystem = "windows")]

use std::env;
use std::process::Command;

fn main() {
    let exe_path = env::current_exe().expect("Failed to get executable path");
    let root_dir = exe_path.parent().expect("Executable has no parent");

    let real_exe = root_dir.join("lib").join("hfm-player.exe");

    if !real_exe.exists() {
        eprintln!("Error: Core binary not found at {:?}", real_exe);
        std::process::exit(1);
    }

    let lib_dir = root_dir.join("lib");
    let current_path = env::var("PATH").unwrap_or_default();
    let new_path = format!("{};{}", lib_dir.display(), current_path);

    let status = Command::new(real_exe)
        .env("PATH", new_path)
        .status()
        .expect("Failed to launch core binary");

    std::process::exit(status.code().unwrap_or(1));
}
