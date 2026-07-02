use crate::clustering::Cluster;

/// Shannon entropy (natural log, nats) over the cluster-size distribution —
/// high entropy on *meaning* clusters (not phrasing) is the confabulation
/// signal from the semantic-entropy literature. `0.0` when every sample
/// landed in one cluster (fully consistent); grows with more, more evenly
/// sized clusters.
pub fn semantic_entropy(clusters: &[Cluster], total: usize) -> f32 {
    if total == 0 {
        return 0.0;
    }
    clusters
        .iter()
        .map(|c| c.members.len() as f32 / total as f32)
        .filter(|p| *p > 0.0)
        .map(|p| -p * p.ln())
        .sum()
}

/// Opportunistic, single-generation confidence proxy: mean per-token log
/// probability from a response's `logprobs.content[].logprob` array, when
/// the backend returns one (verified present on this project's local
/// Ollama instance — see `.pro/research/confidence-signals.md`). Returns
/// `None` when the field is absent, which is expected on backends that
/// don't support it — never treated as a hard requirement.
pub fn logprob_confidence(raw_response: &serde_json::Value) -> Option<f32> {
    let tokens = raw_response
        .get("choices")?
        .get(0)?
        .get("logprobs")?
        .get("content")?
        .as_array()?;
    if tokens.is_empty() {
        return None;
    }
    let sum: f64 = tokens.iter().filter_map(|t| t.get("logprob")?.as_f64()).sum();
    Some((sum / tokens.len() as f64) as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_cluster_has_zero_entropy() {
        let clusters = vec![Cluster { members: vec![0, 1, 2] }];
        assert_eq!(semantic_entropy(&clusters, 3), 0.0);
    }

    #[test]
    fn evenly_split_clusters_have_higher_entropy_than_skewed() {
        let even = vec![Cluster { members: vec![0] }, Cluster { members: vec![1] }];
        let skewed = vec![Cluster { members: vec![0, 1, 2, 3] }, Cluster { members: vec![4] }];
        let even_entropy = semantic_entropy(&even, 2);
        let skewed_entropy = semantic_entropy(&skewed, 5);
        assert!(even_entropy > skewed_entropy);
    }

    #[test]
    fn logprob_confidence_extracts_mean_from_valid_response() {
        let raw = serde_json::json!({
            "choices": [{
                "logprobs": {
                    "content": [
                        {"logprob": -0.5},
                        {"logprob": -1.5}
                    ]
                }
            }]
        });
        assert_eq!(logprob_confidence(&raw), Some(-1.0));
    }

    #[test]
    fn logprob_confidence_is_none_when_absent() {
        let raw = serde_json::json!({ "choices": [{ "message": { "content": "hi" } }] });
        assert_eq!(logprob_confidence(&raw), None);
    }
}
