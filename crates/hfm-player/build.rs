use std::fs;
use std::path::Path;

fn main() {
    let shader_path = "src/renderer/shaders/texture_quad.wgsl";

    // Tell cargo to rerun if the shader changes.
    println!("cargo:rerun-if-changed={}", shader_path);

    // Validate the shader if it exists.
    if Path::new(shader_path).exists() {
        let source = fs::read_to_string(shader_path).expect("Failed to read shader file");
        if let Err(e) = naga::front::wgsl::parse_str(&source) {
            panic!("Shader validation failed in {}: {}", shader_path, e);
        }
        println!("cargo:rustc-cfg=shader_validated");
    } else {
        // In development, the shader might not be present; we can skip validation.
        println!("cargo:warning=Shader file not found: {}", shader_path);
    }
}
