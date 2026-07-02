use std::path::PathBuf;

use pro_audit::AuditEvent;
use pro_provider::{Role, Segment};
use serde::{Deserialize, Serialize};

use crate::{Engine, EngineError, PromptAssembler, ToolAdapter};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposedFile {
    pub path: PathBuf,
    pub old_content: Option<String>,
    pub new_content: String,
    /// Unified diff against `old_content` (empty original if the file is
    /// new) — computed here via `similar`, never hand-authored by the
    /// model. Asking the model for whole-file new content instead of raw
    /// diff syntax matches "structured output as the default for internal
    /// agent steps" (`10-cost-optimization.md` §7): more reliable to
    /// generate, and this crate does the diffing deterministically.
    pub diff: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProposedChange {
    pub files: Vec<ProposedFile>,
}

#[derive(Debug, Deserialize)]
struct RawProposedFile {
    path: String,
    new_content: String,
}

#[derive(Debug, Deserialize)]
struct RawProposal {
    files: Vec<RawProposedFile>,
}

fn response_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "files": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" },
                        "new_content": { "type": "string" }
                    },
                    "required": ["path", "new_content"]
                }
            }
        },
        "required": ["files"]
    })
}

impl Engine {
    /// The `Propose` step: reads the current content of every file in
    /// `scope_paths` (capability-gated via `fs_read`, same as any other
    /// filesystem access), asks the provider for the complete new content
    /// of whichever files need to change, and diffs the result against
    /// what's on disk. Doesn't write anything — that's a separate,
    /// explicitly capability-gated `fs_write` step.
    pub async fn propose_diff(&self, task: &str, scope_paths: &[PathBuf]) -> Result<ProposedChange, EngineError> {
        let mut context_files = Vec::with_capacity(scope_paths.len());
        for path in scope_paths {
            let bytes = self.fs_read(path)?;
            context_files.push((path.clone(), String::from_utf8_lossy(&bytes).to_string()));
        }

        let mut context_text = String::new();
        for (path, content) in &context_files {
            context_text.push_str(&format!("--- {} ---\n{content}\n\n", path.display()));
        }

        let request_base = PromptAssembler::assemble(
            "You are pro's codegen step. Given a task and the current content of relevant files, propose the \
             complete new content for each file that needs to change. Respond only with the structured output \
             requested — full file contents, never diffs or partial snippets.",
            &context_text,
            vec![Segment::volatile(Role::User, format!("Task: {task}"))],
            Some(response_schema()),
        );

        let response = self.provider.complete(&request_base).await?;
        let parsed: RawProposal = serde_json::from_str(&response.content)?;

        let files = parsed
            .files
            .into_iter()
            .map(|raw| {
                let path = PathBuf::from(&raw.path);
                let old_content = context_files.iter().find(|(p, _)| p == &path).map(|(_, c)| c.clone());
                let diff = similar::TextDiff::from_lines(old_content.as_deref().unwrap_or(""), &raw.new_content)
                    .unified_diff()
                    .header(&format!("a/{}", raw.path), &format!("b/{}", raw.path))
                    .to_string();
                ProposedFile { path, old_content, new_content: raw.new_content, diff }
            })
            .collect::<Vec<_>>();

        let event = AuditEvent::new(
            "engine",
            "provider",
            "propose_diff",
            "Allowed",
            format!("proposed changes to {} file(s) for task: {task}", files.len()),
            serde_json::json!({ "files": files.iter().map(|f| f.path.display().to_string()).collect::<Vec<_>>() }),
        );
        let _ = self.audit.append(&event);

        Ok(ProposedChange { files })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use pro_audit::AuditLog;
    use pro_capability::{Capability, Grant, GrantStore};
    use pro_provider::{CompletionRequest, CompletionResponse, Provider, ProviderError};
    use pro_sandbox::EnforcerRegistry;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct ScriptedProvider {
        reply: String,
    }

    #[async_trait]
    impl Provider for ScriptedProvider {
        async fn complete(&self, _req: &CompletionRequest) -> Result<CompletionResponse, ProviderError> {
            Ok(CompletionResponse { content: self.reply.clone(), raw: serde_json::json!({}) })
        }
    }

    fn temp_audit_path(name: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("pro-core-propose-test-{name}-{nonce}.jsonl"))
    }

    fn test_config() -> pro_config::Config {
        pro_config::Config {
            provider: pro_config::ProviderConfig {
                name: "ollama".into(),
                base_url: "http://localhost:11434/v1".into(),
                api_key_env: String::new(),
                model: "test-model".into(),
            },
            profile: pro_config::ProfileConfig { default: pro_config::ProfileName::ReadOnly },
            audit: pro_config::AuditConfig { log_path: "unused".into(), retention_days: 90 },
            cascade_provider: None,
        }
    }

    #[tokio::test]
    async fn propose_diff_computes_a_real_unified_diff_against_disk_content() {
        let dir = std::env::temp_dir().join(format!("pro-core-propose-test-dir-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file_path = dir.join("lib.rs");
        std::fs::write(&file_path, "fn old() {}\n").unwrap();

        let mut grants = GrantStore::new();
        grants.add_grant(Grant::new_shadow(Capability::FsRead { paths: vec![dir.clone()] }));

        let reply = serde_json::json!({
            "files": [{ "path": file_path.to_string_lossy(), "new_content": "fn new() {}\n" }]
        })
        .to_string();
        let provider = ScriptedProvider { reply };

        let audit_path = temp_audit_path("compute-diff");
        let engine =
            Engine::new(test_config(), grants, AuditLog::new(audit_path.clone()), EnforcerRegistry::new(), Box::new(provider));

        let change = engine.propose_diff("rename old to new", std::slice::from_ref(&file_path)).await.unwrap();
        assert_eq!(change.files.len(), 1);
        assert_eq!(change.files[0].old_content.as_deref(), Some("fn old() {}\n"));
        assert!(change.files[0].diff.contains("-fn old"));
        assert!(change.files[0].diff.contains("+fn new"));

        std::fs::remove_dir_all(&dir).ok();
        std::fs::remove_file(&audit_path).ok();
    }

    #[tokio::test]
    async fn propose_diff_denies_reading_files_outside_the_granted_scope() {
        let audit_path = temp_audit_path("denied-scope");
        let provider = ScriptedProvider { reply: serde_json::json!({ "files": [] }).to_string() };
        let engine = Engine::new(
            test_config(),
            GrantStore::new(),
            AuditLog::new(audit_path.clone()),
            EnforcerRegistry::new(),
            Box::new(provider),
        );

        let result = engine.propose_diff("do something", &[std::path::PathBuf::from("/etc/passwd")]).await;
        assert!(matches!(result, Err(EngineError::CapabilityDenied(_))));

        std::fs::remove_file(&audit_path).ok();
    }
}
