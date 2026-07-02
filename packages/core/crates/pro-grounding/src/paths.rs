/// Extracts the contents of every string literal in `source` — a
/// tokenizer-free heuristic (no `regex` dependency needed for this),
/// language-agnostic since Rust, TS, and JS all use `"`/`'` string
/// delimiters with backslash escaping.
pub fn extract_string_literals(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = source.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '"' || c == '\'' {
            let quote = c;
            let mut literal = String::new();
            let mut closed = false;
            for next in chars.by_ref() {
                if next == '\\' {
                    continue;
                }
                if next == quote {
                    closed = true;
                    break;
                }
                literal.push(next);
            }
            if closed {
                out.push(literal);
            }
        }
    }
    out
}

/// Whether a string literal looks like a filesystem path worth checking —
/// deliberately conservative (skips URLs, skips anything with no path
/// separator) to keep the false-positive rate low on this heuristic.
pub fn looks_like_path(literal: &str) -> bool {
    if literal.is_empty() || literal.starts_with("http://") || literal.starts_with("https://") {
        return false;
    }
    literal.contains('/') || literal.contains('\\')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_double_and_single_quoted_literals() {
        let literals = extract_string_literals(r#"let a = "foo/bar"; let b = 'baz/qux';"#);
        assert_eq!(literals, vec!["foo/bar".to_string(), "baz/qux".to_string()]);
    }

    #[test]
    fn path_like_excludes_urls_and_plain_words() {
        assert!(looks_like_path("src/lib.rs"));
        assert!(!looks_like_path("https://example.com/thing"));
        assert!(!looks_like_path("hello"));
    }
}
