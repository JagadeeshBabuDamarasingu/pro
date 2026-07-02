use std::path::PathBuf;

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    Rust,
    TypeScript,
    JavaScript,
}

impl Language {
    pub fn of_path(path: &std::path::Path) -> Option<Language> {
        match path.extension().and_then(|e| e.to_str()) {
            Some("rs") => Some(Language::Rust),
            Some("ts") | Some("tsx") => Some(Language::TypeScript),
            Some("js") | Some("jsx") | Some("mjs") => Some(Language::JavaScript),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Symbol {
    pub name: String,
    pub kind: &'static str,
    pub file: PathBuf,
    pub line: usize,
}

/// A referenced (called) identifier, with its 0-indexed source position —
/// the position is what lets `pro-grounding`'s LSP tier ask
/// `textDocument/definition` at the exact call site, not just "does this
/// name exist somewhere."
#[derive(Debug, Clone)]
pub struct Reference {
    pub name: String,
    pub line: usize,
    pub character: usize,
}

/// The result of parsing one source file: what it defines, and what
/// identifiers it references — deliberately name-based rather than fully
/// scope-resolved (that's what the LSP-backed grounding gate is for; this
/// is the free, always-on, zero-LLM-cost tier).
#[derive(Debug, Clone, Default)]
pub struct ParsedFile {
    pub symbols: Vec<Symbol>,
    pub references: Vec<Reference>,
}
