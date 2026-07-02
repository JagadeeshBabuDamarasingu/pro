use pro_provider::{CompletionRequest, Provider, ProviderError, Role, Segment};
use serde::Serialize;

use crate::similarity::jaccard_similarity;

#[derive(Debug, Clone, Serialize)]
pub struct ReviewResult {
    pub mode_used: &'static str,
    pub verdict: String,
    /// Intermediate reasoning steps — empty for `simple`, four entries
    /// (CLAIM/EXTRACT/DOUBT/RECONCILE) for `adversarial`.
    pub trail: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OrthogonalReviewResult {
    pub primary: ReviewResult,
    pub secondary: ReviewResult,
    /// `Some(reason)` when the two independent reviewers meaningfully
    /// diverge — surfaced for a human decision, never auto-resolved
    /// (`02-security-core.md` §4).
    pub disagreement: Option<String>,
}

const SIMPLE_SYSTEM_PROMPT: &str =
    "You review a code diff for correctness and safety issues. Be specific — cite file:line where possible. \
     Be concise.";

const ADVERSARIAL_SYSTEM_PROMPT: &str =
    "You are reviewing a code diff before it's finalized, using a structured doubt-driven process: CLAIM, then \
     EXTRACT, then DOUBT, then RECONCILE. Follow each instruction exactly, one step at a time, building on your \
     own prior answers.";

async fn call_step(
    provider: &dyn Provider,
    system: &str,
    conversation: &mut Vec<Segment>,
) -> Result<String, ProviderError> {
    let request = CompletionRequest {
        stable_segments: vec![Segment::stable(Role::System, system)],
        volatile_segments: conversation.clone(),
        response_schema: None,
        max_tokens: None,
        temperature: None,
        logprobs: false,
    };
    let response = provider.complete(&request).await?;
    conversation.push(Segment::volatile(Role::Assistant, response.content.clone()));
    Ok(response.content)
}

/// One fast pass — the default review mode for everyday commits.
pub async fn simple(provider: &dyn Provider, diff: &str) -> Result<ReviewResult, ProviderError> {
    let mut conversation = vec![Segment::volatile(Role::User, diff.to_string())];
    let verdict = call_step(provider, SIMPLE_SYSTEM_PROMPT, &mut conversation).await?;
    Ok(ReviewResult { mode_used: "simple", verdict, trail: Vec::new() })
}

/// CLAIM → EXTRACT → DOUBT → RECONCILE → STOP, in-flight on the diff
/// before it's finalized — Osmani's doubt-driven-development pattern
/// (`02-security-core.md` §4), for security-sensitive or irreversible
/// changes.
pub async fn adversarial(provider: &dyn Provider, diff: &str) -> Result<ReviewResult, ProviderError> {
    let mut conversation = vec![Segment::volatile(
        Role::User,
        format!("Diff:\n{diff}\n\nCLAIM: state in one paragraph what you believe this diff does and why it's correct."),
    )];
    let mut trail = Vec::with_capacity(4);

    let claim = call_step(provider, ADVERSARIAL_SYSTEM_PROMPT, &mut conversation).await?;
    trail.push(format!("CLAIM: {claim}"));

    conversation.push(Segment::volatile(
        Role::User,
        "EXTRACT: list the specific file:line evidence from the diff that backs your claim.",
    ));
    let extract = call_step(provider, ADVERSARIAL_SYSTEM_PROMPT, &mut conversation).await?;
    trail.push(format!("EXTRACT: {extract}"));

    conversation.push(Segment::volatile(
        Role::User,
        "DOUBT: now argue against your own claim — find the strongest reason it could be wrong.",
    ));
    let doubt = call_step(provider, ADVERSARIAL_SYSTEM_PROMPT, &mut conversation).await?;
    trail.push(format!("DOUBT: {doubt}"));

    conversation.push(Segment::volatile(
        Role::User,
        "RECONCILE: weigh the claim against the doubt and give a final verdict — does this diff have a real \
         issue, or is the doubt unfounded? STOP after your verdict.",
    ));
    let reconcile = call_step(provider, ADVERSARIAL_SYSTEM_PROMPT, &mut conversation).await?;
    trail.push(format!("RECONCILE: {reconcile}"));

    Ok(ReviewResult { mode_used: "adversarial", verdict: reconcile, trail })
}

/// Routes the diff through two independently-configured providers with no
/// shared context (`primary`/`secondary` never see each other's call) —
/// heterogeneous reviewers catch disjoint bugs that homogeneous multi-
/// sampling misses (Osmani's Greptile/CodeRabbit/Seer finding). Disagreement
/// is surfaced, never auto-resolved.
pub async fn orthogonal(
    primary_provider: &dyn Provider,
    secondary_provider: &dyn Provider,
    diff: &str,
) -> Result<OrthogonalReviewResult, ProviderError> {
    let primary = simple(primary_provider, diff).await?;
    let secondary = simple(secondary_provider, diff).await?;

    let similarity = jaccard_similarity(&primary.verdict, &secondary.verdict);
    let disagreement = (similarity < 0.2).then(|| {
        format!(
            "primary and secondary reviewers diverge significantly (token-overlap similarity {similarity:.2}) — \
             human judgment required, not auto-resolved"
        )
    });

    Ok(OrthogonalReviewResult {
        primary: ReviewResult { mode_used: "orthogonal-primary", ..primary },
        secondary: ReviewResult { mode_used: "orthogonal-secondary", ..secondary },
        disagreement,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use pro_provider::CompletionResponse;
    use std::sync::Mutex;

    struct ScriptedProvider {
        replies: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl Provider for ScriptedProvider {
        async fn complete(&self, _req: &CompletionRequest) -> Result<CompletionResponse, ProviderError> {
            let mut replies = self.replies.lock().unwrap();
            let reply = if replies.len() > 1 { replies.remove(0) } else { replies[0].clone() };
            Ok(CompletionResponse { content: reply, raw: serde_json::json!({}) })
        }
    }

    #[tokio::test]
    async fn simple_review_returns_single_pass_with_empty_trail() {
        let provider = ScriptedProvider { replies: Mutex::new(vec!["looks fine".to_string()]) };
        let result = simple(&provider, "diff --git a/x.rs b/x.rs").await.unwrap();
        assert_eq!(result.mode_used, "simple");
        assert_eq!(result.verdict, "looks fine");
        assert!(result.trail.is_empty());
    }

    #[tokio::test]
    async fn adversarial_review_produces_four_step_trail() {
        let provider = ScriptedProvider {
            replies: Mutex::new(vec![
                "claim text".to_string(),
                "extract text".to_string(),
                "doubt text".to_string(),
                "reconcile text".to_string(),
            ]),
        };
        let result = adversarial(&provider, "diff --git a/x.rs b/x.rs").await.unwrap();
        assert_eq!(result.mode_used, "adversarial");
        assert_eq!(result.trail.len(), 4);
        assert!(result.trail[0].starts_with("CLAIM:"));
        assert!(result.trail[3].starts_with("RECONCILE:"));
        assert_eq!(result.verdict, "reconcile text");
    }

    #[tokio::test]
    async fn orthogonal_review_flags_disagreement_between_divergent_verdicts() {
        let primary = ScriptedProvider { replies: Mutex::new(vec!["no issues found here at all".to_string()]) };
        let secondary = ScriptedProvider { replies: Mutex::new(vec!["critical security vulnerability present".to_string()]) };
        let result = orthogonal(&primary, &secondary, "diff --git a/x.rs b/x.rs").await.unwrap();
        assert!(result.disagreement.is_some());
    }

    #[tokio::test]
    async fn orthogonal_review_no_disagreement_on_matching_verdicts() {
        let primary = ScriptedProvider { replies: Mutex::new(vec!["looks good no issues".to_string()]) };
        let secondary = ScriptedProvider { replies: Mutex::new(vec!["looks good no issues".to_string()]) };
        let result = orthogonal(&primary, &secondary, "diff --git a/x.rs b/x.rs").await.unwrap();
        assert!(result.disagreement.is_none());
    }
}
