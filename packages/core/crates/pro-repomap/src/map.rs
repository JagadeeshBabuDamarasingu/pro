use std::collections::HashMap;
use std::path::{Path, PathBuf};

use petgraph::graph::{DiGraph, NodeIndex};

use crate::pagerank::pagerank;
use crate::parse::parse_file;
use crate::symbol::Symbol;
use crate::walk::source_files;

/// Name -> defining-file lookup, first-definer-wins on name collisions.
/// Backs `pro-grounding`'s AST/symbol-table fallback path.
pub struct SymbolIndex {
    definers: HashMap<String, PathBuf>,
}

impl SymbolIndex {
    pub fn contains(&self, name: &str) -> bool {
        self.definers.contains_key(name)
    }

    pub fn defined_in(&self, name: &str) -> Option<&Path> {
        self.definers.get(name).map(PathBuf::as_path)
    }
}

/// Aider-style repo map: files as nodes, "file A references a symbol
/// defined in file B" as weighted edges, ranked by PageRank. Name-based
/// symbol resolution (first-definer-wins on collisions) rather than full
/// scope resolution — zero LLM cost, real precision trade-off documented
/// here rather than hidden; `pro-grounding`'s LSP tier exists precisely to
/// cover what this can't.
pub struct RepoMap {
    file_index: HashMap<PathBuf, NodeIndex>,
    file_symbols: HashMap<PathBuf, Vec<Symbol>>,
    symbol_index: SymbolIndex,
    ranks: HashMap<NodeIndex, f64>,
}

impl RepoMap {
    pub fn build(root: &Path) -> RepoMap {
        let files = source_files(root);

        let mut parsed_by_file = HashMap::new();
        for file in &files {
            // Skip files this pass can't parse (unsupported syntax, IO
            // error) rather than failing the whole repo map — one bad file
            // shouldn't deny context for the rest of the repo.
            if let Ok(parsed) = parse_file(file) {
                parsed_by_file.insert(file.clone(), parsed);
            }
        }

        let mut definers: HashMap<String, PathBuf> = HashMap::new();
        for (file, parsed) in &parsed_by_file {
            for symbol in &parsed.symbols {
                definers.entry(symbol.name.clone()).or_insert_with(|| file.clone());
            }
        }

        let mut graph: DiGraph<PathBuf, u32> = DiGraph::new();
        let mut file_index = HashMap::new();
        for file in parsed_by_file.keys() {
            file_index.insert(file.clone(), graph.add_node(file.clone()));
        }

        let mut edge_weights: HashMap<(NodeIndex, NodeIndex), u32> = HashMap::new();
        for (file, parsed) in &parsed_by_file {
            let from = file_index[file];
            for reference in &parsed.references {
                if let Some(target_file) = definers.get(&reference.name) {
                    if target_file != file {
                        let to = file_index[target_file];
                        *edge_weights.entry((from, to)).or_insert(0) += 1;
                    }
                }
            }
        }
        for ((from, to), weight) in edge_weights {
            graph.add_edge(from, to, weight);
        }

        let ranks = pagerank(&graph, 0.85, 40);
        let file_symbols: HashMap<PathBuf, Vec<Symbol>> =
            parsed_by_file.into_iter().map(|(f, p)| (f, p.symbols)).collect();

        RepoMap { file_index, file_symbols, symbol_index: SymbolIndex { definers }, ranks }
    }

    pub fn symbol_index(&self) -> &SymbolIndex {
        &self.symbol_index
    }

    /// Top-ranked files first, symbols listed underneath, elided once the
    /// char/4-estimated token budget is spent. No real tokenizer — this is
    /// a free, always-on context mechanism, not a billing input, so an
    /// approximate budget is the right level of precision.
    pub fn render(&self, budget_tokens: usize) -> String {
        let mut ranked: Vec<(&PathBuf, f64)> =
            self.file_index.iter().map(|(f, idx)| (f, *self.ranks.get(idx).unwrap_or(&0.0))).collect();
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let mut out = String::new();
        let mut approx_tokens = 0usize;
        for (file, rank) in ranked {
            let symbols = &self.file_symbols[file];
            if symbols.is_empty() {
                continue;
            }
            let header = format!("{} (rank {rank:.4})\n", file.display());
            approx_tokens += header.len() / 4;
            if approx_tokens > budget_tokens {
                break;
            }
            out.push_str(&header);
            for symbol in symbols {
                let line = format!("  {} {} :{}\n", symbol.kind, symbol.name, symbol.line);
                approx_tokens += line.len() / 4;
                if approx_tokens > budget_tokens {
                    return out;
                }
                out.push_str(&line);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn scratch_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pro-repomap-map-test-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(dir: &Path, name: &str, contents: &str) {
        let mut file = std::fs::File::create(dir.join(name)).unwrap();
        file.write_all(contents.as_bytes()).unwrap();
    }

    #[test]
    fn cross_file_reference_ranks_definer_above_isolated_file() {
        let dir = scratch_dir("crossref");
        write(&dir, "lib.rs", "fn helper() {}\n");
        write(&dir, "main.rs", "fn run() { helper(); }\n");
        write(&dir, "isolated.rs", "fn unused() {}\n");

        let map = RepoMap::build(&dir);
        let rendered = map.render(10_000);
        let lib_pos = rendered.find("lib.rs").expect("lib.rs listed");
        let isolated_pos = rendered.find("isolated.rs").expect("isolated.rs listed");
        assert!(lib_pos < isolated_pos, "definer file should rank above the never-referenced file");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn symbol_index_resolves_defined_symbol() {
        let dir = scratch_dir("symidx");
        write(&dir, "lib.rs", "fn helper() {}\n");

        let map = RepoMap::build(&dir);
        assert!(map.symbol_index().contains("helper"));
        assert!(map.symbol_index().defined_in("helper").unwrap().ends_with("lib.rs"));
        assert!(!map.symbol_index().contains("does_not_exist"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn render_respects_token_budget() {
        let dir = scratch_dir("budget");
        write(&dir, "lib.rs", "fn a() {}\nfn b() {}\nfn c() {}\n");

        let map = RepoMap::build(&dir);
        let full = map.render(10_000);
        let truncated = map.render(1);
        assert!(truncated.len() <= full.len());

        std::fs::remove_dir_all(&dir).ok();
    }
}
