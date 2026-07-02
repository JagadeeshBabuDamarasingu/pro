use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::diff::FileDiff;

/// Fraction of added lines that duplicate another added line elsewhere in
/// the diff (exact match after whitespace trimming) — a cheap, real proxy
/// for GitClear's copy-pasted-code-share finding. `0.0` is no duplication.
pub fn duplication_ratio(files: &[FileDiff]) -> f32 {
    let added: Vec<&str> = files
        .iter()
        .flat_map(|f| f.hunks.iter())
        .flat_map(|h| h.added_lines.iter())
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect();
    if added.is_empty() {
        return 0.0;
    }
    let mut seen: HashSet<&str> = HashSet::new();
    let duplicates = added.iter().filter(|l| !seen.insert(l)).count();
    duplicates as f32 / added.len() as f32
}

/// Fraction of touched files that fall under a declared scope — `1.0` when
/// no scope is declared (nothing to violate) or the diff touches no files.
pub fn scope_discipline(files: &[FileDiff], declared_scope: &[PathBuf]) -> f32 {
    if files.is_empty() || declared_scope.is_empty() {
        return 1.0;
    }
    let in_scope = files.iter().filter(|f| declared_scope.iter().any(|s| Path::new(&f.path).starts_with(s))).count();
    in_scope as f32 / files.len() as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::Hunk;

    fn file(path: &str, added: &[&str]) -> FileDiff {
        FileDiff {
            path: path.into(),
            hunks: vec![Hunk { added_lines: added.iter().map(|s| s.to_string()).collect(), removed_lines: vec![] }],
        }
    }

    #[test]
    fn no_duplication_is_zero() {
        let files = vec![file("a.rs", &["let x = 1;", "let y = 2;"])];
        assert_eq!(duplication_ratio(&files), 0.0);
    }

    #[test]
    fn repeated_line_across_files_is_flagged() {
        let files = vec![file("a.rs", &["let x = 1;"]), file("b.rs", &["let x = 1;"])];
        assert_eq!(duplication_ratio(&files), 0.5);
    }

    #[test]
    fn scope_discipline_flags_out_of_scope_touch() {
        let files = vec![file("src/auth/login.rs", &["x"]), file("src/billing/invoice.rs", &["y"])];
        let scope = vec![PathBuf::from("src/auth")];
        assert_eq!(scope_discipline(&files, &scope), 0.5);
    }

    #[test]
    fn no_declared_scope_is_always_full_score() {
        let files = vec![file("anywhere.rs", &["x"])];
        assert_eq!(scope_discipline(&files, &[]), 1.0);
    }
}
