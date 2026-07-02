use pro_provider::{CompletionRequest, CompletionResponse, Provider, ProviderError};

use crate::clustering::ClusteringStrategy;
use crate::entropy::semantic_entropy;
use crate::gate::{gate, ConfidenceReport, ConfidenceVerdict};
use crate::sample::sample_n;

#[derive(Debug, Clone)]
pub struct CascadeOutcome {
    pub response: CompletionResponse,
    pub escalated: bool,
    pub confidence: ConfidenceReport,
}

/// Model cascade using the confidence-signal gate as the escalation trigger
/// (`10-cost-optimization.md` §3): try the cheap/local model first, and
/// only pay for the strong/hosted model when the cheap model's own answers
/// don't agree with each other on what they mean.
pub struct Cascade {
    pub cheap: Box<dyn Provider>,
    pub strong: Box<dyn Provider>,
}

impl Cascade {
    pub async fn ask_cascaded(
        &self,
        request: &CompletionRequest,
        strategy: &dyn ClusteringStrategy,
        sample_count: usize,
        temperature: f32,
        entropy_threshold: f32,
    ) -> Result<CascadeOutcome, ProviderError> {
        let samples = sample_n(self.cheap.as_ref(), request, sample_count, temperature).await?;
        let clusters = strategy.cluster(&samples).await?;
        let entropy = semantic_entropy(&clusters, samples.len());
        let confidence = ConfidenceReport {
            entropy,
            num_clusters: clusters.len(),
            strategy: strategy.name(),
            logprob_proxy: samples.first().and_then(|s| crate::entropy::logprob_confidence(&s.raw)),
        };

        match gate(&confidence, entropy_threshold) {
            ConfidenceVerdict::Confident => {
                // Pick the largest cluster's first sample — majority-vote
                // selection over the meaning-clusters, not just the first
                // sample drawn.
                let largest = clusters.iter().max_by_key(|c| c.members.len()).expect("at least one cluster");
                let chosen = &samples[largest.members[0]];
                Ok(CascadeOutcome {
                    response: CompletionResponse { content: chosen.text.clone(), raw: chosen.raw.clone() },
                    escalated: false,
                    confidence,
                })
            }
            ConfidenceVerdict::LowConfidence { .. } => {
                let response = self.strong.complete(request).await?;
                Ok(CascadeOutcome { response, escalated: true, confidence })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clustering::LexicalConsistency;
    use async_trait::async_trait;
    use pro_provider::{Role, Segment};

    struct ScriptedProvider {
        replies: std::sync::Mutex<Vec<String>>,
    }

    #[async_trait]
    impl Provider for ScriptedProvider {
        async fn complete(&self, _req: &CompletionRequest) -> Result<CompletionResponse, ProviderError> {
            let mut replies = self.replies.lock().unwrap();
            let reply = if replies.len() > 1 { replies.remove(0) } else { replies[0].clone() };
            Ok(CompletionResponse { content: reply, raw: serde_json::json!({}) })
        }
    }

    fn request() -> CompletionRequest {
        CompletionRequest {
            stable_segments: vec![Segment::stable(Role::System, "sys")],
            volatile_segments: vec![Segment::volatile(Role::User, "question")],
            response_schema: None,
            max_tokens: None,
            temperature: None,
            logprobs: false,
        }
    }

    #[tokio::test]
    async fn consistent_cheap_answers_never_escalate() {
        let cheap = ScriptedProvider { replies: std::sync::Mutex::new(vec!["paris".to_string()]) };
        let strong = ScriptedProvider { replies: std::sync::Mutex::new(vec!["should not be called".to_string()]) };
        let cascade = Cascade { cheap: Box::new(cheap), strong: Box::new(strong) };
        let strategy = LexicalConsistency::default();

        let outcome = cascade.ask_cascaded(&request(), &strategy, 3, 0.7, 0.5).await.unwrap();
        assert!(!outcome.escalated);
        assert_eq!(outcome.response.content, "paris");
    }

    #[tokio::test]
    async fn disagreeing_cheap_answers_escalate_to_strong() {
        let cheap = ScriptedProvider {
            replies: std::sync::Mutex::new(vec!["paris".to_string(), "london".to_string(), "tokyo".to_string()]),
        };
        let strong = ScriptedProvider { replies: std::sync::Mutex::new(vec!["paris (verified)".to_string()]) };
        let cascade = Cascade { cheap: Box::new(cheap), strong: Box::new(strong) };
        let strategy = LexicalConsistency::default();

        let outcome = cascade.ask_cascaded(&request(), &strategy, 3, 0.7, 0.1).await.unwrap();
        assert!(outcome.escalated);
        assert_eq!(outcome.response.content, "paris (verified)");
    }
}
