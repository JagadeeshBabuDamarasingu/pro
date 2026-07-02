use std::path::{Path, PathBuf};

const SKIP_DIRS: &[&str] = &["target", "node_modules", ".git", ".turbo", ".pro"];

/// Recursively collects source files under `root` with a language this
/// crate knows how to parse, skipping build/dependency directories.
/// Hand-rolled rather than a `walkdir` dependency — the skip-list and
/// extension filter are the only real logic needed here.
pub fn source_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    walk_into(root, &mut files);
    files
}

fn walk_into(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if SKIP_DIRS.contains(&name) {
                continue;
            }
            walk_into(&path, out);
        } else if crate::symbol::Language::of_path(&path).is_some() {
            out.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_target_directory() {
        let root = std::env::temp_dir().join(format!("pro-repomap-walk-test-{}", std::process::id()));
        std::fs::create_dir_all(root.join("target")).unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("target/generated.rs"), "fn x() {}").unwrap();
        std::fs::write(root.join("src/lib.rs"), "fn y() {}").unwrap();

        let files = source_files(&root);
        assert!(files.iter().any(|f| f.ends_with("src/lib.rs")));
        assert!(!files.iter().any(|f| f.to_string_lossy().contains("target")));

        std::fs::remove_dir_all(&root).ok();
    }
}
