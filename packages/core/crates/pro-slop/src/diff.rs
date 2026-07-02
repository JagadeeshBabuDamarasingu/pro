#[derive(Debug, Clone, Default)]
pub struct Hunk {
    pub added_lines: Vec<String>,
    pub removed_lines: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct FileDiff {
    pub path: String,
    pub hunks: Vec<Hunk>,
}

/// Minimal unified-diff parser — just enough structure (touched files,
/// added/removed lines per hunk) for the heuristics below. Hand-rolled
/// rather than pulling in a diff-parsing crate for this small a surface.
pub fn parse(diff_text: &str) -> Vec<FileDiff> {
    let mut files: Vec<FileDiff> = Vec::new();
    let mut current_path: Option<String> = None;
    let mut hunks: Vec<Hunk> = Vec::new();
    let mut current_hunk: Option<Hunk> = None;

    fn flush_hunk(hunks: &mut Vec<Hunk>, current_hunk: &mut Option<Hunk>) {
        if let Some(h) = current_hunk.take() {
            hunks.push(h);
        }
    }

    fn flush_file(files: &mut Vec<FileDiff>, path: &mut Option<String>, hunks: &mut Vec<Hunk>) {
        if let Some(path) = path.take() {
            if !hunks.is_empty() {
                files.push(FileDiff { path, hunks: std::mem::take(hunks) });
            }
        }
        hunks.clear();
    }

    for line in diff_text.lines() {
        if let Some(rest) = line.strip_prefix("+++ ") {
            flush_hunk(&mut hunks, &mut current_hunk);
            flush_file(&mut files, &mut current_path, &mut hunks);
            let path = rest.strip_prefix("b/").unwrap_or(rest).to_string();
            if path != "/dev/null" {
                current_path = Some(path);
            }
        } else if line.starts_with("--- ") {
            continue;
        } else if line.starts_with("@@") {
            flush_hunk(&mut hunks, &mut current_hunk);
            current_hunk = Some(Hunk::default());
        } else if let Some(hunk) = current_hunk.as_mut() {
            if let Some(added) = line.strip_prefix('+') {
                hunk.added_lines.push(added.to_string());
            } else if let Some(removed) = line.strip_prefix('-') {
                hunk.removed_lines.push(removed.to_string());
            }
        }
    }
    flush_hunk(&mut hunks, &mut current_hunk);
    flush_file(&mut files, &mut current_path, &mut hunks);
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n\
--- a/src/lib.rs\n\
+++ b/src/lib.rs\n\
@@ -1,3 +1,4 @@\n\
 fn main() {\n\
+    println!(\"added\");\n\
-    println!(\"removed\");\n\
 }\n";

    #[test]
    fn parses_touched_file_path() {
        let files = parse(SAMPLE_DIFF);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "src/lib.rs");
    }

    #[test]
    fn parses_added_and_removed_lines() {
        let files = parse(SAMPLE_DIFF);
        let hunk = &files[0].hunks[0];
        assert_eq!(hunk.added_lines, vec!["    println!(\"added\");"]);
        assert_eq!(hunk.removed_lines, vec!["    println!(\"removed\");"]);
    }

    #[test]
    fn empty_diff_yields_no_files() {
        assert!(parse("").is_empty());
    }
}
