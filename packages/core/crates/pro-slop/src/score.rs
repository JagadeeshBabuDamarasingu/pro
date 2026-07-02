use std::path::PathBuf;

use pro_provider::{Provider, ProviderError};

use crate::diff::parse;
use crate::heuristics::{duplication_ratio, scope_discipline};
use crate::judge::judge_diff;
use crate::report::{SlopAxis, SlopReport};

/// Runs the deterministic heuristics always; adds the LLM-judge axes only
/// when a `provider` is supplied (they cost an API call, so the caller
/// opts in — mirrors `pro-cli`'s `--full` flag on `pro slop`).
pub async fn score(
    provider: Option<&dyn Provider>,
    diff_text: &str,
    declared_scope: &[PathBuf],
) -> Result<SlopReport, ProviderError> {
    let files = parse(diff_text);
    let mut axis_scores = vec![
        (SlopAxis::MechanicalCleanliness, 1.0 - duplication_ratio(&files)),
        (SlopAxis::ScopeDiscipline, scope_discipline(&files, declared_scope)),
    ];
    let mut findings = Vec::new();

    if let Some(provider) = provider {
        let (judged_scores, judged_findings) = judge_diff(provider, diff_text).await?;
        axis_scores.extend(judged_scores);
        findings.extend(judged_findings);
    }

    let overall = SlopReport::compute_overall(&axis_scores);
    Ok(SlopReport { axis_scores, findings, overall })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn heuristics_only_report_has_no_findings_without_a_provider() {
        let diff = "+++ b/a.rs\n@@ -0,0 +1 @@\n+let x = 1;\n";
        let report = score(None, diff, &[]).await.unwrap();
        assert!(report.findings.is_empty());
        assert_eq!(report.axis_scores.len(), 2);
    }
}
