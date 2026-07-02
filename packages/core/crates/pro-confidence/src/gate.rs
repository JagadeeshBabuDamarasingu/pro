#[derive(Debug, Clone, serde::Serialize)]
pub struct ConfidenceReport {
    pub entropy: f32,
    pub num_clusters: usize,
    pub strategy: &'static str,
    pub logprob_proxy: Option<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConfidenceVerdict {
    Confident,
    LowConfidence { reason: String },
}

/// Low confidence never routes to silently displaying the output — it
/// routes to escalation (the model cascade) or an explicit "verify
/// manually" flag, per `02-security-core.md` §4.
pub fn gate(report: &ConfidenceReport, entropy_threshold: f32) -> ConfidenceVerdict {
    if report.entropy > entropy_threshold {
        ConfidenceVerdict::LowConfidence {
            reason: format!(
                "{} split {} samples into {} meaning-clusters (entropy {:.3} > threshold {:.3})",
                report.strategy, report.num_clusters, report.num_clusters, report.entropy, entropy_threshold
            ),
        }
    } else {
        ConfidenceVerdict::Confident
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn low_entropy_is_confident() {
        let report = ConfidenceReport { entropy: 0.0, num_clusters: 1, strategy: "lexical_consistency", logprob_proxy: None };
        assert_eq!(gate(&report, 0.5), ConfidenceVerdict::Confident);
    }

    #[test]
    fn high_entropy_is_low_confidence() {
        let report = ConfidenceReport { entropy: 1.2, num_clusters: 3, strategy: "lexical_consistency", logprob_proxy: None };
        assert!(matches!(gate(&report, 0.5), ConfidenceVerdict::LowConfidence { .. }));
    }
}
