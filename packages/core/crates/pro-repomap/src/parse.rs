use std::path::Path;

use tree_sitter::{Query, QueryCursor, StreamingIterator};

use crate::symbol::{Language, ParsedFile, Symbol};

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("unsupported file extension: {0}")]
    UnsupportedLanguage(String),
    #[error("tree-sitter failed to set language: {0}")]
    Language(#[from] tree_sitter::LanguageError),
    #[error("tree-sitter failed to build query: {0}")]
    Query(#[from] tree_sitter::QueryError),
    #[error("tree-sitter failed to parse the file")]
    ParseFailed,
    #[error("failed to read file: {0}")]
    Io(#[from] std::io::Error),
}

/// One tree-sitter query per language, tagging definitions with
/// `def.<kind>`/`def.name` capture pairs and references with `ref.call`.
/// Node names verified against the real grammars (not guessed) — see
/// `.pro/research/` conventions from Phase 0/2 for why that matters.
fn query_source(language: Language) -> &'static str {
    match language {
        Language::Rust => {
            "(function_item name: (identifier) @def.name) @def.function
             (struct_item name: (type_identifier) @def.name) @def.struct
             (enum_item name: (type_identifier) @def.name) @def.enum
             (trait_item name: (type_identifier) @def.name) @def.trait
             (function_signature_item name: (identifier) @def.name) @def.function
             (call_expression function: (identifier) @ref.call)
             (call_expression function: (field_expression field: (field_identifier) @ref.call))
             (call_expression function: (scoped_identifier name: (identifier) @ref.call))"
        }
        Language::TypeScript => {
            "(function_declaration name: (identifier) @def.name) @def.function
             (class_declaration name: (type_identifier) @def.name) @def.class
             (method_definition name: (property_identifier) @def.name) @def.method
             (interface_declaration name: (type_identifier) @def.name) @def.interface
             (call_expression function: (identifier) @ref.call)
             (call_expression function: (member_expression property: (property_identifier) @ref.call))"
        }
        Language::JavaScript => {
            "(function_declaration name: (identifier) @def.name) @def.function
             (class_declaration name: (identifier) @def.name) @def.class
             (method_definition name: (property_identifier) @def.name) @def.method
             (call_expression function: (identifier) @ref.call)
             (call_expression function: (member_expression property: (property_identifier) @ref.call))"
        }
    }
}

fn ts_language(language: Language) -> tree_sitter::Language {
    match language {
        Language::Rust => tree_sitter_rust::LANGUAGE.into(),
        Language::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        Language::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
    }
}

/// Parses one source file into its defined symbols and referenced
/// identifiers. Name-based, not scope-resolved — a reference to `helper`
/// doesn't yet know *which* `helper` it means if the repo defines more than
/// one; `RepoMap`'s edge-building step resolves that with a simple
/// first-definer-wins rule, documented there.
pub fn parse_file(path: &Path) -> Result<ParsedFile, ParseError> {
    let language = Language::of_path(path)
        .ok_or_else(|| ParseError::UnsupportedLanguage(path.display().to_string()))?;
    let source = std::fs::read_to_string(path)?;
    parse_source(&source, language, path)
}

/// Parses in-memory source (no file read) — used by `pro-grounding` to
/// check *proposed* content before it's ever written to disk.
pub fn parse_source(source: &str, language: Language, path: &Path) -> Result<ParsedFile, ParseError> {
    let ts_lang = ts_language(language);
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&ts_lang)?;
    let tree = parser.parse(source, None).ok_or(ParseError::ParseFailed)?;

    let query = Query::new(&ts_lang, query_source(language))?;
    let capture_names = query.capture_names();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());

    let mut parsed = ParsedFile::default();
    while let Some(m) = matches.next() {
        for cap in m.captures {
            let capture_name = capture_names[cap.index as usize];
            let text = source[cap.node.byte_range()].to_string();
            let position = cap.node.start_position();
            let line = position.row + 1;

            if capture_name == "ref.call" {
                parsed.references.push(crate::symbol::Reference {
                    name: text,
                    line: position.row,
                    character: position.column,
                });
            } else if capture_name == "def.name" {
                // The kind lives on the sibling capture with the same
                // match (e.g. `def.function`); look it up from this same
                // match's captures rather than a second query pass.
                let kind = m
                    .captures
                    .iter()
                    .map(|c| capture_names[c.index as usize])
                    .find(|n| n.starts_with("def.") && *n != "def.name")
                    .unwrap_or("def.symbol");
                parsed.symbols.push(Symbol {
                    name: text,
                    kind: kind_str(kind),
                    file: path.to_path_buf(),
                    line,
                });
            }
        }
    }

    Ok(parsed)
}

fn kind_str(capture_name: &str) -> &'static str {
    match capture_name {
        "def.function" => "function",
        "def.struct" => "struct",
        "def.enum" => "enum",
        "def.trait" => "trait",
        "def.class" => "class",
        "def.method" => "method",
        "def.interface" => "interface",
        _ => "symbol",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_temp(name: &str, contents: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(name);
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(contents.as_bytes()).unwrap();
        path
    }

    #[test]
    fn extracts_rust_function_and_call_reference() {
        let path = write_temp("pro-repomap-test-rust.rs", "fn helper(x: i32) -> i32 { x }\nfn other() { helper(1); }\n");
        let parsed = parse_file(&path).unwrap();
        assert!(parsed.symbols.iter().any(|s| s.name == "helper" && s.kind == "function"));
        assert!(parsed.symbols.iter().any(|s| s.name == "other" && s.kind == "function"));
        assert!(parsed.references.iter().any(|r| r.name == "helper"));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn extracts_typescript_class_and_method() {
        let path = write_temp(
            "pro-repomap-test-ts.ts",
            "class Foo {\n  method() {\n    helper();\n  }\n}\nfunction helper() {}\n",
        );
        let parsed = parse_file(&path).unwrap();
        assert!(parsed.symbols.iter().any(|s| s.name == "Foo" && s.kind == "class"));
        assert!(parsed.symbols.iter().any(|s| s.name == "method" && s.kind == "method"));
        assert!(parsed.references.iter().any(|r| r.name == "helper"));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn unsupported_extension_errors() {
        let path = std::path::PathBuf::from("/tmp/whatever.py");
        assert!(matches!(parse_file(&path), Err(ParseError::UnsupportedLanguage(_))));
    }
}
