use std::path::Path;
use std::time::Duration;

use pro_lsp::LspClient;
use pro_repomap::RepoMap;

use crate::finding::{GroundingFinding, GroundingReport, HallucinationCategory};
use crate::paths::{extract_string_literals, looks_like_path};

/// `Scalar` always carries exactly one location; `Array`/`Link` carry zero
/// or more — an empty list is how rust-analyzer (and LSP servers
/// generally) represent "no definition found," not `null`. Treating
/// `Some(Array([]))` as "resolved" was a real bug caught during live
/// verification against a real rust-analyzer session, not a hypothetical.
fn definition_response_is_empty(response: &lsp_types::GotoDefinitionResponse) -> bool {
    match response {
        lsp_types::GotoDefinitionResponse::Scalar(_) => false,
        lsp_types::GotoDefinitionResponse::Array(locations) => locations.is_empty(),
        lsp_types::GotoDefinitionResponse::Link(links) => links.is_empty(),
    }
}

/// The mandatory pre-display grounding gate (`02-security-core.md` §4):
/// deterministically checks referenced paths/symbols in *proposed* content
/// against the real filesystem and codebase, before it's ever shown.
pub struct GroundingGate<'a> {
    repo_map: &'a RepoMap,
}

impl<'a> GroundingGate<'a> {
    pub fn new(repo_map: &'a RepoMap) -> Self {
        GroundingGate { repo_map }
    }

    fn check_paths(&self, base_dir: &Path, new_content: &str) -> Vec<GroundingFinding> {
        extract_string_literals(new_content)
            .into_iter()
            .filter(|literal| looks_like_path(literal))
            .filter(|literal| !base_dir.join(literal).exists())
            .map(|literal| GroundingFinding {
                category: HallucinationCategory::Resource,
                file: base_dir.to_path_buf(),
                symbol_or_path: literal.clone(),
                detail: format!("referenced path '{literal}' does not exist relative to {}", base_dir.display()),
            })
            .collect()
    }

    /// AST/symbol-table fallback tier: checks referenced calls against the
    /// repo map's own symbol index. Real, documented false-positive rate on
    /// external/stdlib calls (no semantic type info at this tier) — that's
    /// exactly the gap the LSP tier exists to close for Rust.
    fn check_symbols_fallback(&self, file_path: &Path, new_content: &str) -> Vec<GroundingFinding> {
        let Some(language) = pro_repomap::Language::of_path(file_path) else {
            return Vec::new();
        };
        let Ok(parsed) = pro_repomap::parse_source(new_content, language, file_path) else {
            return Vec::new();
        };
        parsed
            .references
            .into_iter()
            .filter(|r| !self.repo_map.symbol_index().contains(&r.name))
            .map(|r| GroundingFinding {
                category: HallucinationCategory::Naming,
                file: file_path.to_path_buf(),
                symbol_or_path: r.name.clone(),
                detail: format!(
                    "'{}' not found in the local symbol index at {}:{} (AST-only fallback — may be a valid \
                     external/stdlib call this tier can't see)",
                    r.name,
                    r.line + 1,
                    r.character
                ),
            })
            .collect()
    }

    /// LSP-backed tier, Rust only: opens the proposed content as a virtual
    /// document and asks the real language server whether each referenced
    /// call resolves — `textDocument/definition` as ground truth, not the
    /// model's training-time memory of similar-looking APIs.
    async fn check_symbols_lsp(
        &self,
        file_path: &Path,
        new_content: &str,
        lsp: &LspClient,
        uri: lsp_types::Uri,
    ) -> Vec<GroundingFinding> {
        let Some(language) = pro_repomap::Language::of_path(file_path) else {
            return Vec::new();
        };
        if language != pro_repomap::Language::Rust {
            return self.check_symbols_fallback(file_path, new_content);
        }
        let Ok(parsed) = pro_repomap::parse_source(new_content, language, file_path) else {
            return Vec::new();
        };

        if lsp.did_open(uri.clone(), "rust", new_content.to_string()).is_err() {
            return self.check_symbols_fallback(file_path, new_content);
        }
        // Give rust-analyzer a moment to index the freshly opened document.
        lsp.wait_for_diagnostics(&uri, Duration::from_secs(5)).await;

        let mut findings = Vec::new();
        for reference in parsed.references {
            let position = lsp_types::Position { line: reference.line as u32, character: reference.character as u32 };
            match lsp.definition(uri.clone(), position).await {
                Ok(Some(response)) if !definition_response_is_empty(&response) => {}
                // Both `None` and an empty `Array`/`Link` response mean
                // "no definition found" — rust-analyzer uses an empty
                // array for this, not `null`, so `Some(_)` alone isn't
                // enough to conclude the reference resolved.
                Ok(_) => findings.push(GroundingFinding {
                    category: HallucinationCategory::Naming,
                    file: file_path.to_path_buf(),
                    symbol_or_path: reference.name.clone(),
                    detail: format!(
                        "rust-analyzer found no definition for '{}' at {}:{}",
                        reference.name,
                        reference.line + 1,
                        reference.character
                    ),
                }),
                // A transport hiccup (timeout/protocol error) isn't
                // evidence of a hallucination — don't turn an
                // infrastructure failure into a false finding.
                Err(_) => {}
            }
        }
        findings
    }

    /// Runs both path and symbol checks. `lsp` is `Some((client, uri))`
    /// for the Rust LSP-backed tier; `None` falls back to AST/symbol-table
    /// for every language, which is the *only* tier TS/JS gets in this
    /// pass.
    pub async fn check(
        &self,
        file_path: &Path,
        base_dir: &Path,
        new_content: &str,
        lsp: Option<(&LspClient, lsp_types::Uri)>,
    ) -> GroundingReport {
        let mut findings = self.check_paths(base_dir, new_content);
        match lsp {
            Some((client, uri)) => findings.extend(self.check_symbols_lsp(file_path, new_content, client, uri).await),
            None => findings.extend(self.check_symbols_fallback(file_path, new_content)),
        }
        GroundingReport { findings }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn scratch_repo(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("pro-grounding-test-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(dir: &Path, name: &str, contents: &str) {
        let mut file = std::fs::File::create(dir.join(name)).unwrap();
        file.write_all(contents.as_bytes()).unwrap();
    }

    #[test]
    fn empty_array_response_counts_as_no_definition() {
        // Regression test: a real rust-analyzer session returns
        // `Array([])`, not `None`, when nothing resolves — caught via live
        // verification, not a hypothetical edge case.
        assert!(definition_response_is_empty(&lsp_types::GotoDefinitionResponse::Array(vec![])));
        assert!(definition_response_is_empty(&lsp_types::GotoDefinitionResponse::Link(vec![])));
    }

    #[test]
    fn non_empty_response_counts_as_resolved() {
        let location = lsp_types::Location {
            uri: "file:///tmp/x.rs".parse().unwrap(),
            range: lsp_types::Range::default(),
        };
        assert!(!definition_response_is_empty(&lsp_types::GotoDefinitionResponse::Scalar(location.clone())));
        assert!(!definition_response_is_empty(&lsp_types::GotoDefinitionResponse::Array(vec![location])));
    }

    #[tokio::test]
    async fn flags_nonexistent_referenced_path() {
        let dir = scratch_repo("path");
        write(&dir, "lib.rs", "fn helper() {}\n");
        let repo_map = RepoMap::build(&dir);
        let gate = GroundingGate::new(&repo_map);

        let proposed = r#"fn f() { std::fs::read("definitely/does/not/exist.txt"); }"#;
        let report = gate.check(&dir.join("new.rs"), &dir, proposed, None).await;
        assert!(report.findings.iter().any(|f| f.category == HallucinationCategory::Resource));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn fallback_tier_allows_locally_defined_symbol() {
        let dir = scratch_repo("localsym");
        write(&dir, "lib.rs", "fn helper() {}\n");
        let repo_map = RepoMap::build(&dir);
        let gate = GroundingGate::new(&repo_map);

        let proposed = "fn f() { helper(); }";
        let report = gate.check(&dir.join("new.rs"), &dir, proposed, None).await;
        assert!(!report.findings.iter().any(|f| f.symbol_or_path == "helper"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn fallback_tier_flags_nonexistent_local_symbol() {
        let dir = scratch_repo("hallucinated");
        write(&dir, "lib.rs", "fn helper() {}\n");
        let repo_map = RepoMap::build(&dir);
        let gate = GroundingGate::new(&repo_map);

        let proposed = "fn f() { totally_made_up_function_name(); }";
        let report = gate.check(&dir.join("new.rs"), &dir, proposed, None).await;
        assert!(report
            .findings
            .iter()
            .any(|f| f.category == HallucinationCategory::Naming && f.symbol_or_path == "totally_made_up_function_name"));

        std::fs::remove_dir_all(&dir).ok();
    }
}
