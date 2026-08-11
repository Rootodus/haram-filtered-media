use std::fs;

fn main() {
    let shader_dir = "examples/shaders/";

    if let Ok(entries) = fs::read_dir(shader_dir) {
        for entry in entries.flatten() {
            let path = entry.path();

            if path.extension().map_or(false, |e| e == "wgsl") {
                println!("cargo:rerun-if-changed={}", path.display());

                let source = fs::read_to_string(&path).unwrap();
                if let Err(e) = naga::front::wgsl::parse_str(&source) {
                    panic!("Shader validation failed in {}: {}", path.display(), e);
                }
            }
        }
    }
}
