use std::collections::HashSet;

use async_trait::async_trait;
use pro_provider::{CompletionRequest, Provider, ProviderError, Role, Segment};

use crate::sample::Sample;

#[derive(Debug, Clone)]
pub struct Cluster {
    pub members: Vec<usize>,
}

#[async_trait]
pub trait ClusteringStrategy: Send + Sync {
    fn name(&self) -> &'static str;
    async fn cluster(&self, samples: &[Sample]) -> Result<Vec<Cluster>, ProviderError>;
}

fn token_set(text: &str) -> HashSet<String> {
    text.to_lowercase().split_whitespace().map(str::to_string).collect()
}

fn jaccard_similarity(a: &HashSet<String>, b: &HashSet<String>) -> f32 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    let intersection = a.intersection(b).count();
    let union = a.union(b).count().max(1);
    intersection as f32 / union as f32
}

/// SelfCheckGPT-style fallback: clusters by surface-text (token-set)
/// agreement, no extra API calls beyond the samples themselves. Weaker
/// signal than `EntailmentClustering` — consistent wrong phrasing can look
/// like consistent right phrasing — but free.
pub struct LexicalConsistency {
    pub similarity_threshold: f32,
}

impl Default for LexicalConsistency {
    fn default() -> Self {
        LexicalConsistency { similarity_threshold: 0.5 }
    }
}

#[async_trait]
impl ClusteringStrategy for LexicalConsistency {
    fn name(&self) -> &'static str {
        "lexical_consistency"
    }

    async fn cluster(&self, samples: &[Sample]) -> Result<Vec<Cluster>, ProviderError> {
        let token_sets: Vec<HashSet<String>> = samples.iter().map(|s| token_set(&s.text)).collect();
        let mut clusters: Vec<Cluster> = Vec::new();
        for (idx, tokens) in token_sets.iter().enumerate() {
            let home = clusters.iter_mut().find(|c| {
                let rep = c.members[0];
                jaccard_similarity(tokens, &token_sets[rep]) >= self.similarity_threshold
            });
            match home {
                Some(cluster) => cluster.members.push(idx),
                None => clusters.push(Cluster { members: vec![idx] }),
            }
        }
        Ok(clusters)
    }
}

/// The real, full Semantic Entropy method (Kuhn et al. 2023 / Farquhar et
/// al. 2024): clusters by meaning-equivalence via a structured-output
/// entailment judge call through the same `Provider` abstraction, rather
/// than surface text. See `.pro/research/confidence-signals.md` for why
/// this — and not the hidden-state "Probe" variant — is what's buildable
/// over an HTTP-only provider.
pub struct EntailmentClustering<'a> {
    pub provider: &'a dyn Provider,
}

impl<'a> EntailmentClustering<'a> {
    async fn same_claim(&self, a: &str, b: &str) -> Result<bool, ProviderError> {
        let schema = serde_json::json!({
            "type": "object",
            "properties": { "same_claim": { "type": "boolean" } },
            "required": ["same_claim"]
        });
        let request = CompletionRequest {
            stable_segments: vec![Segment::stable(
                Role::System,
                "You judge whether two answers assert the same underlying claim, ignoring \
                 differences in phrasing. Respond only with the structured field requested.",
            )],
            volatile_segments: vec![Segment::volatile(Role::User, format!("Answer A: {a}\nAnswer B: {b}"))],
            response_schema: Some(schema),
            max_tokens: None,
            temperature: None,
            logprobs: false,
        };
        let response = self.provider.complete(&request).await?;
        #[derive(serde::Deserialize)]
        struct Judgement {
            same_claim: bool,
        }
        Ok(serde_json::from_str::<Judgement>(&response.content).map(|j| j.same_claim).unwrap_or(false))
    }
}

#[async_trait]
impl<'a> ClusteringStrategy for EntailmentClustering<'a> {
    fn name(&self) -> &'static str {
        "entailment_clustering"
    }

    async fn cluster(&self, samples: &[Sample]) -> Result<Vec<Cluster>, ProviderError> {
        let mut clusters: Vec<Cluster> = Vec::new();
        for (idx, sample) in samples.iter().enumerate() {
            let mut placed = false;
            for cluster in &mut clusters {
                let rep = &samples[cluster.members[0]];
                if self.same_claim(&sample.text, &rep.text).await? {
                    cluster.members.push(idx);
                    placed = true;
                    break;
                }
            }
            if !placed {
                clusters.push(Cluster { members: vec![idx] });
            }
        }
        Ok(clusters)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(text: &str) -> Sample {
        Sample { text: text.into(), raw: serde_json::json!({}) }
    }

    #[tokio::test]
    async fn lexical_consistency_groups_similar_phrasing() {
        let samples = vec![sample("the sky is blue"), sample("the sky is very blue"), sample("bananas are yellow")];
        let strategy = LexicalConsistency::default();
        let clusters = strategy.cluster(&samples).await.unwrap();
        assert_eq!(clusters.len(), 2);
    }

    #[tokio::test]
    async fn lexical_consistency_all_identical_is_one_cluster() {
        let samples = vec![sample("42"), sample("42"), sample("42")];
        let strategy = LexicalConsistency::default();
        let clusters = strategy.cluster(&samples).await.unwrap();
        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0].members.len(), 3);
    }
}
