use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segment {
    pub role: Role,
    pub content: String,
    /// Signals that this segment is a good prompt-cache breakpoint.
    /// `OpenAiCompatibleClient` doesn't need to act on this explicitly —
    /// ordering stable segments first already lets prefix-caching (OpenAI's
    /// automatic prefix cache, Ollama/llama.cpp's context-prefix reuse)
    /// kick in — but a provider with an explicit `cache_control` knob
    /// (e.g. Anthropic's Messages API) would read this field.
    pub cache_hint: bool,
}

impl Segment {
    pub fn stable(role: Role, content: impl Into<String>) -> Self {
        Segment { role, content: content.into(), cache_hint: true }
    }

    pub fn volatile(role: Role, content: impl Into<String>) -> Self {
        Segment { role, content: content.into(), cache_hint: false }
    }
}

/// A completion request already split into stability-ordered segments.
/// `stable_segments` (system prompt, tool/capability manifest) must be
/// assembled ahead of `volatile_segments` (conversation, retrieved
/// content) — see `PromptAssembler` in `pro-core`, which is what actually
/// produces requests in this shape; this struct is the provider-facing
/// contract that ordering flows into.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionRequest {
    pub stable_segments: Vec<Segment>,
    pub volatile_segments: Vec<Segment>,
    /// JSON-schema-constrained output for internal agent steps
    /// (structured-output-by-default per `10-cost-optimization.md`).
    pub response_schema: Option<serde_json::Value>,
    pub max_tokens: Option<u32>,
    /// Sampling temperature. Used by `pro-confidence`'s repeat-sampling
    /// signals (SelfCheckGPT-style consistency, semantic-entropy
    /// clustering), which need nonzero temperature to get meaningfully
    /// different generations across `n` samples.
    pub temperature: Option<f32>,
    /// Requests per-token log probabilities on the response, when the
    /// backend supports it (verified present on this project's local Ollama
    /// instance — see `.pro/research/confidence-signals.md`). Ignored by
    /// backends that don't support it; `OpenAiCompatibleClient` just passes
    /// it through.
    pub logprobs: bool,
}

#[derive(Debug, Clone)]
pub struct CompletionResponse {
    pub content: String,
    pub raw: serde_json::Value,
}

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("request to provider failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("provider returned status {status}: {body}")]
    UnexpectedStatus { status: u16, body: String },
    #[error("provider response had no completion choice")]
    MissingChoice,
}

#[async_trait]
pub trait Provider: Send + Sync {
    async fn complete(&self, req: &CompletionRequest) -> Result<CompletionResponse, ProviderError>;
}

/// The one client that serves hosted OpenAI-compatible providers and local
/// Ollama identically — per `01-architecture.md`, swapping models/engines
/// is a config change (base_url + optional key), never a code change.
pub struct OpenAiCompatibleClient {
    http: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
    model: String,
}

impl OpenAiCompatibleClient {
    pub fn new(base_url: impl Into<String>, api_key: Option<String>, model: impl Into<String>) -> Self {
        OpenAiCompatibleClient {
            http: reqwest::Client::new(),
            base_url: base_url.into(),
            api_key,
            model: model.into(),
        }
    }

    fn to_chat_message(segment: &Segment) -> serde_json::Value {
        let role = match segment.role {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
        };
        serde_json::json!({ "role": role, "content": segment.content })
    }
}

#[async_trait]
impl Provider for OpenAiCompatibleClient {
    async fn complete(&self, req: &CompletionRequest) -> Result<CompletionResponse, ProviderError> {
        let messages: Vec<serde_json::Value> = req
            .stable_segments
            .iter()
            .chain(req.volatile_segments.iter())
            .map(Self::to_chat_message)
            .collect();

        let mut body = serde_json::json!({
            "model": self.model,
            "messages": messages,
        });
        if let Some(schema) = &req.response_schema {
            body["response_format"] = serde_json::json!({
                "type": "json_schema",
                "json_schema": schema,
            });
        }
        if let Some(max_tokens) = req.max_tokens {
            body["max_tokens"] = serde_json::json!(max_tokens);
        }
        if let Some(temperature) = req.temperature {
            body["temperature"] = serde_json::json!(temperature);
        }
        if req.logprobs {
            body["logprobs"] = serde_json::json!(true);
        }

        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let mut request = self.http.post(url).json(&body);
        if let Some(key) = &self.api_key {
            if !key.is_empty() {
                request = request.bearer_auth(key);
            }
        }

        let response = request.send().await?;
        let status = response.status();
        let raw: serde_json::Value = if status.is_success() {
            response.json().await?
        } else {
            let text = response.text().await.unwrap_or_default();
            return Err(ProviderError::UnexpectedStatus { status: status.as_u16(), body: text });
        };

        let content = raw
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .ok_or(ProviderError::MissingChoice)?
            .to_string();

        Ok(CompletionResponse { content, raw })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_segments_precede_volatile_in_message_order() {
        let req = CompletionRequest {
            stable_segments: vec![Segment::stable(Role::System, "system prompt")],
            volatile_segments: vec![Segment::volatile(Role::User, "hello")],
            response_schema: None,
            max_tokens: None,
            temperature: None,
            logprobs: false,
        };
        let ordered: Vec<&Segment> = req.stable_segments.iter().chain(req.volatile_segments.iter()).collect();
        assert_eq!(ordered[0].content, "system prompt");
        assert_eq!(ordered[1].content, "hello");
    }

    #[test]
    fn chat_message_maps_role_correctly() {
        let msg = OpenAiCompatibleClient::to_chat_message(&Segment::stable(Role::System, "hi"));
        assert_eq!(msg["role"], "system");
        assert_eq!(msg["content"], "hi");
    }
}
