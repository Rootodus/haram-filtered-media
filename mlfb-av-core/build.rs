use std::fs;

fn main() {
    let shader_dir = "examples/shaders/";

    // 1. Tell Cargo to watch the folder itself (catches additions/deletions)
    println!("cargo:rerun-if-changed={}", shader_dir);

    for entry in fs::read_dir(shader_dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().map_or(false, |e| e == "wgsl") {
            // 2. Tell Cargo to watch this specific file (catches edits)
            println!("cargo:rerun-if-changed={}", path.display());

            let source = fs::read_to_string(&path).unwrap();
            if let Err(e) = naga::front::wgsl::parse_str(&source) {
                panic!("Shader validation failed in {}: {}", path.display(), e);
            }
        }
    }
}
