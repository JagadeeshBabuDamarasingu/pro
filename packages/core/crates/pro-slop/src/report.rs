/// FrontierCode's mergeability axes (`02-security-core.md` §4) — adopted
/// near-verbatim rather than inventing a bespoke rubric.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SlopAxis {
    BehavioralCorrectness,
    RegressionSafety,
    MechanicalCleanliness,
    TestCorrectness,
    ScopeDiscipline,
    ConventionConformance,
}

/// CodeHalu's four-category hallucination taxonomy, used to tag findings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CodeHaluCategory {
    Mapping,
    Naming,
    Resource,
    Logical,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SlopFinding {
    pub category: CodeHaluCategory,
    pub axis: SlopAxis,
    pub description: String,
}

/// `axis_scores` is a list, not a map: the deterministic heuristics and the
/// LLM-judge can both independently score the same axis (e.g.
/// `MechanicalCleanliness` from the duplication heuristic *and* from the
/// judge's own read) — that's intentional, not a dedup bug, since the two
/// scores carry different information (cheap-and-certain vs. judged).
#[derive(Debug, Clone, serde::Serialize)]
pub struct SlopReport {
    pub axis_scores: Vec<(SlopAxis, f32)>,
    pub findings: Vec<SlopFinding>,
    pub overall: f32,
}

impl SlopReport {
    pub fn compute_overall(axis_scores: &[(SlopAxis, f32)]) -> f32 {
        if axis_scores.is_empty() {
            return 0.0;
        }
        axis_scores.iter().map(|(_, score)| *score).sum::<f32>() / axis_scores.len() as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overall_is_mean_of_axis_scores() {
        let scores = vec![(SlopAxis::ScopeDiscipline, 1.0), (SlopAxis::MechanicalCleanliness, 0.5)];
        assert_eq!(SlopReport::compute_overall(&scores), 0.75);
    }

    #[test]
    fn empty_scores_is_zero_not_a_panic() {
        assert_eq!(SlopReport::compute_overall(&[]), 0.0);
    }
}
