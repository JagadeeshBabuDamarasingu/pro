use std::path::PathBuf;

use serde::Serialize;

/// CodeHalu's four-category hallucination taxonomy (`02-security-core.md`
/// §4). Kept as its own small enum here rather than depending on
/// `pro-slop`'s copy — different pipeline stage (pre-display grounding vs.
/// post-diff quality scoring), same taxonomy; a one-enum cross-crate
/// dependency isn't worth it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HallucinationCategory {
    Mapping,
    Naming,
    Resource,
    Logical,
}

#[derive(Debug, Clone, Serialize)]
pub struct GroundingFinding {
    pub category: HallucinationCategory,
    pub file: PathBuf,
    pub symbol_or_path: String,
    pub detail: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct GroundingReport {
    pub findings: Vec<GroundingFinding>,
}

impl GroundingReport {
    pub fn is_clean(&self) -> bool {
        self.findings.is_empty()
    }
}
