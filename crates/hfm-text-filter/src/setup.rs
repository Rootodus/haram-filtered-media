//! Path resolution and banned words parsing.

use std::path::{Path, PathBuf};

/// Reads a banned words file, stripping empty lines and comments (lines starting with '#').
pub fn parse_banned_words(path: &Path) -> Result<Vec<String>, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {:?}: {}", path, e))?;

    let words: Vec<String> = content
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.to_string())
        .collect();

    if words.is_empty() {
        return Err(format!("{:?} contains no valid words (empty or only comments)", path));
    }

    Ok(words)
}

/// Searches the given candidate directories for the ONNX model and banned_words.txt.
pub fn resolve_config_in_dirs(candidates: &[PathBuf]) -> Result<(PathBuf, Vec<String>), String> {
    for base in candidates {
        let m_path = base.join("models").join("bart-base-detox").join("onnx");
        if !m_path.exists() {
            continue;
        }

        let b_paths = [
            base.join("banned_words.txt"),
            base.join("tools").join("data").join("banned_words.txt"),
        ];

        for b_path in &b_paths {
            if b_path.exists() {
                let words = parse_banned_words(b_path)?;
                return Ok((m_path, words));
            }
        }
    }

    Err("Could not find 'models/bart-base-detox/onnx' and 'banned_words.txt' in any candidate directory.".to_string())
}

/// Resolves the configuration using the standard search order:
/// 1. Executable directory (distribution)
/// 2. Current working directory (development)
/// 3. Crate root (cargo run)
pub fn resolve_config() -> Result<(PathBuf, Vec<String>), String> {
    let mut candidates = Vec::new();

    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            candidates.push(parent.to_path_buf());
        }
    }

    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd);
    }

    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")));

    resolve_config_in_dirs(&candidates)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_parse_banned_words_clean() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("banned.txt");
        fs::write(&path, "word1\nword2\n").unwrap();

        let words = parse_banned_words(&path).unwrap();
        assert_eq!(words, vec!["word1", "word2"]);
    }

    #[test]
    fn test_parse_banned_words_with_comments_and_blanks() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("banned.txt");
        fs::write(&path, "# This is a comment\n\nword1\n  word2  \n# Another comment\nword3\n").unwrap();

        let words = parse_banned_words(&path).unwrap();
        assert_eq!(words, vec!["word1", "word2", "word3"]);
    }

    #[test]
    fn test_parse_banned_words_empty_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("banned.txt");
        fs::write(&path, "# Only comments\n\n").unwrap();

        let result = parse_banned_words(&path);
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_config_in_dirs_success() {
        let dir = tempdir().unwrap();
        let base = dir.path().to_path_buf();

        // Create fake model dir and banned file
        let model_dir = base.join("models").join("bart-base-detox").join("onnx");
        fs::create_dir_all(&model_dir).unwrap();
        fs::write(base.join("banned_words.txt"), "word1").unwrap();

        let (resolved_model, words) = resolve_config_in_dirs(&[base]).unwrap();
        assert_eq!(resolved_model, model_dir);
        assert_eq!(words, vec!["word1"]);
    }

    #[test]
    fn test_resolve_config_in_dirs_fallback_tools_data() {
        let dir = tempdir().unwrap();
        let base = dir.path().to_path_buf();

        // Create fake model dir, but put banned words in tools/data
        let model_dir = base.join("models").join("bart-base-detox").join("onnx");
        fs::create_dir_all(&model_dir).unwrap();
        let tools_data = base.join("tools").join("data");
        fs::create_dir_all(&tools_data).unwrap();
        fs::write(tools_data.join("banned_words.txt"), "word2").unwrap();

        let (resolved_model, words) = resolve_config_in_dirs(&[base]).unwrap();
        assert_eq!(resolved_model, model_dir);
        assert_eq!(words, vec!["word2"]);
    }

    #[test]
    fn test_resolve_config_in_dirs_failure() {
        let dir = tempdir().unwrap();
        let base = dir.path().to_path_buf();

        // Empty directory, nothing to find
        let result = resolve_config_in_dirs(&[base]);
        assert!(result.is_err());
    }
}