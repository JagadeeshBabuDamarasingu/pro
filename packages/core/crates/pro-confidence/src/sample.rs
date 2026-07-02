use pro_provider::{CompletionRequest, Provider, ProviderError};

#[derive(Debug, Clone)]
pub struct Sample {
    pub text: String,
    pub raw: serde_json::Value,
}

/// Draws `n` generations from `provider` at the given `temperature` —
/// shared sampling infrastructure for both confidence signals below and
/// the existing best-of-N verifier-selection pattern. Sequential, not
/// concurrent: keeps this crate free of an async-fan-out dependency for
/// what's a low-N, already-slow-per-call operation.
pub async fn sample_n(
    provider: &dyn Provider,
    request: &CompletionRequest,
    n: usize,
    temperature: f32,
) -> Result<Vec<Sample>, ProviderError> {
    let mut request = request.clone();
    request.temperature = Some(temperature);

    let mut samples = Vec::with_capacity(n);
    for _ in 0..n {
        let response = provider.complete(&request).await?;
        samples.push(Sample { text: response.content, raw: response.raw });
    }
    Ok(samples)
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use pro_provider::{CompletionResponse, Role, Segment};
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct CountingProvider {
        calls: AtomicUsize,
    }

    #[async_trait]
    impl Provider for CountingProvider {
        async fn complete(&self, req: &CompletionRequest) -> Result<CompletionResponse, ProviderError> {
            assert_eq!(req.temperature, Some(0.8));
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(CompletionResponse { content: "sample".into(), raw: serde_json::json!({}) })
        }
    }

    #[tokio::test]
    async fn samples_the_requested_count_at_the_given_temperature() {
        let provider = CountingProvider { calls: AtomicUsize::new(0) };
        let request = CompletionRequest {
            stable_segments: vec![Segment::stable(Role::System, "sys")],
            volatile_segments: vec![Segment::volatile(Role::User, "hi")],
            response_schema: None,
            max_tokens: None,
            temperature: None,
            logprobs: false,
        };
        let samples = sample_n(&provider, &request, 4, 0.8).await.unwrap();
        assert_eq!(samples.len(), 4);
        assert_eq!(provider.calls.load(Ordering::SeqCst), 4);
    }
}
