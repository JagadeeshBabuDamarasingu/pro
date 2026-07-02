use pro_provider::{CompletionRequest, Provider, ProviderError, Role, Segment};
use serde::Deserialize;

use crate::report::{CodeHaluCategory, SlopAxis, SlopFinding};

const JUDGE_PROMPT: &str = "You review a code diff for mergeability, not just correctness. Score each \
axis from 0.0 (bad) to 1.0 (good): behavioral correctness (does it do what it claims), test \
correctness (do the tests assert the right thing, not just pass), mechanical cleanliness (is \
it tidy, free of leftover debug code), and convention conformance (does it match the \
surrounding code's style). List any specific issues found, each tagged with a CodeHalu \
category: mapping (inconsistent name/entity mapping), naming (nonexistent function/class \
names), resource (nonexistent files/libraries), or logical (runs but doesn't satisfy the \
actual requirement).";

fn schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "behavioral_correctness": { "type": "number" },
            "test_correctness": { "type": "number" },
            "mechanical_cleanliness": { "type": "number" },
            "convention_conformance": { "type": "number" },
            "findings": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "category": { "type": "string", "enum": ["mapping", "naming", "resource", "logical"] },
                        "axis": {
                            "type": "string",
                            "enum": [
                                "behavioral_correctness", "regression_safety", "mechanical_cleanliness",
                                "test_correctness", "scope_discipline", "convention_conformance"
                            ]
                        },
                        "description": { "type": "string" }
                    },
                    "required": ["category", "axis", "description"]
                }
            }
        },
        "required": ["behavioral_correctness", "test_correctness", "mechanical_cleanliness", "convention_conformance", "findings"]
    })
}

#[derive(Debug, Deserialize)]
struct RawFinding {
    category: String,
    axis: String,
    description: String,
}

#[derive(Debug, Deserialize)]
struct RawJudgement {
    behavioral_correctness: f32,
    test_correctness: f32,
    mechanical_cleanliness: f32,
    convention_conformance: f32,
    findings: Vec<RawFinding>,
}

fn parse_category(s: &str) -> CodeHaluCategory {
    match s {
        "naming" => CodeHaluCategory::Naming,
        "resource" => CodeHaluCategory::Resource,
        "logical" => CodeHaluCategory::Logical,
        _ => CodeHaluCategory::Mapping,
    }
}

fn parse_axis(s: &str) -> SlopAxis {
    match s {
        "regression_safety" => SlopAxis::RegressionSafety,
        "test_correctness" => SlopAxis::TestCorrectness,
        "scope_discipline" => SlopAxis::ScopeDiscipline,
        "convention_conformance" => SlopAxis::ConventionConformance,
        "behavioral_correctness" => SlopAxis::BehavioralCorrectness,
        _ => SlopAxis::MechanicalCleanliness,
    }
}

/// LLM-judge axes that can't be computed deterministically. Any
/// unparseable field in the model's structured output degrades to a
/// conservative default (score `0.0`, category `Mapping`) rather than
/// erroring the whole report — a judge call is inherently best-effort.
pub async fn judge_diff(
    provider: &dyn Provider,
    diff_text: &str,
) -> Result<(Vec<(SlopAxis, f32)>, Vec<SlopFinding>), ProviderError> {
    let request = CompletionRequest {
        stable_segments: vec![Segment::stable(Role::System, JUDGE_PROMPT)],
        volatile_segments: vec![Segment::volatile(Role::User, diff_text)],
        response_schema: Some(schema()),
        max_tokens: None,
        temperature: None,
        logprobs: false,
    };
    let response = provider.complete(&request).await?;
    let parsed: RawJudgement = serde_json::from_str(&response.content).unwrap_or(RawJudgement {
        behavioral_correctness: 0.0,
        test_correctness: 0.0,
        mechanical_cleanliness: 0.0,
        convention_conformance: 0.0,
        findings: Vec::new(),
    });

    let axis_scores = vec![
        (SlopAxis::BehavioralCorrectness, parsed.behavioral_correctness),
        (SlopAxis::TestCorrectness, parsed.test_correctness),
        (SlopAxis::MechanicalCleanliness, parsed.mechanical_cleanliness),
        (SlopAxis::ConventionConformance, parsed.convention_conformance),
    ];
    let findings = parsed
        .findings
        .into_iter()
        .map(|f| SlopFinding { category: parse_category(&f.category), axis: parse_axis(&f.axis), description: f.description })
        .collect();

    Ok((axis_scores, findings))
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use pro_provider::CompletionResponse;

    struct StubProvider {
        reply: String,
    }

    #[async_trait]
    impl Provider for StubProvider {
        async fn complete(&self, _req: &CompletionRequest) -> Result<CompletionResponse, ProviderError> {
            Ok(CompletionResponse { content: self.reply.clone(), raw: serde_json::json!({}) })
        }
    }

    #[tokio::test]
    async fn parses_well_formed_judgement() {
        let reply = serde_json::json!({
            "behavioral_correctness": 0.9,
            "test_correctness": 0.5,
            "mechanical_cleanliness": 0.8,
            "convention_conformance": 0.7,
            "findings": [
                { "category": "naming", "axis": "behavioral_correctness", "description": "calls a nonexistent function" }
            ]
        })
        .to_string();
        let provider = StubProvider { reply };
        let (scores, findings) = judge_diff(&provider, "diff text").await.unwrap();
        assert_eq!(scores.len(), 4);
        assert_eq!(findings.len(), 1);
        assert!(matches!(findings[0].category, CodeHaluCategory::Naming));
    }

    #[tokio::test]
    async fn unparseable_reply_degrades_to_conservative_defaults() {
        let provider = StubProvider { reply: "not json".into() };
        let (scores, findings) = judge_diff(&provider, "diff text").await.unwrap();
        assert!(scores.iter().all(|(_, s)| *s == 0.0));
        assert!(findings.is_empty());
    }
}
