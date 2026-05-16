use flatbuffers_build::BuilderOptions;
use std::env;
use std::process::Command;

fn main() {
    let schema_path = "schema.fbs";

    // Locate flatc binary
    let flatc_path = env::var("FLATC")
        .ok()
        .filter(|p| !p.is_empty())
        .or_else(|| which::which("flatc").ok().map(|p| p.to_string_lossy().to_string()))
        .expect(
            "flatc binary not found. Please set FLATC environment variable or ensure flatc.exe is in your PATH.",
        );

    println!("cargo:rerun-if-changed={}", schema_path);

    // Generate Rust bindings using flatbuffers-build
    BuilderOptions::new_with_files([schema_path])
        .set_compiler(&flatc_path)
        .compile()
        .expect("Failed to compile flatbuffer schema for Rust");

    // Generate TypeScript bindings (flatc 25.12+ uses --ts)
    let ts_out_dir = "loader/generated";
    std::fs::create_dir_all(ts_out_dir).expect("Failed to create TS output directory");

    let status = Command::new(&flatc_path)
        .args(&["--ts", "-o", ts_out_dir, schema_path])
        .status()
        .expect("Failed to execute flatc for TypeScript generation");

    if !status.success() {
        let output = Command::new(&flatc_path)
            .args(&["--ts", "-o", ts_out_dir, schema_path])
            .output()
            .expect("Failed to get flatc output");
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        panic!(
            "flatc failed for TypeScript\nstdout:\n{}\nstderr:\n{}",
            stdout, stderr
        );
    }
}
