use std::path::PathBuf;

use async_trait::async_trait;
use pro_orchestrate::{ExecutionContext, ReviewModeSpec, StepExecutor, StepKind, StepOutcome, StepSpec};
use pro_provider::Provider;

use crate::propose::ProposedChange;
use crate::{Engine, ToolAdapter};

/// Executes the fixed Propose → Ground → Verify → Review vocabulary
/// (`pro-orchestrate`) against this workspace's actual machinery —
/// `pro-core` is the composition point the plan calls for, tying
/// `pro-repomap` / `pro-grounding` / `pro-worktree`'s verifier pattern /
/// `pro-review` together. Assumes the fixed step ids from
/// `WorkflowSpec::codegen_pipeline` ("propose", "ground", "verify",
/// "review") — this isn't a generic step-id-agnostic executor.
pub struct WorkflowRunner<'a> {
    engine: &'a Engine,
    repo_map: &'a pro_repomap::RepoMap,
    repo_root: PathBuf,
    lsp: Option<&'a pro_lsp::LspClient>,
    review_secondary: Option<Box<dyn Provider>>,
}

impl<'a> WorkflowRunner<'a> {
    pub fn new(engine: &'a Engine, repo_map: &'a pro_repomap::RepoMap, repo_root: PathBuf) -> Self {
        WorkflowRunner { engine, repo_map, repo_root, lsp: None, review_secondary: None }
    }

    pub fn with_lsp(mut self, lsp: &'a pro_lsp::LspClient) -> Self {
        self.lsp = Some(lsp);
        self
    }

    pub fn with_review_secondary(mut self, provider: Box<dyn Provider>) -> Self {
        self.review_secondary = Some(provider);
        self
    }

    fn proposed_change_from(context: &ExecutionContext, step_id: &str) -> Result<ProposedChange, String> {
        let value = context
            .outputs
            .get("propose")
            .cloned()
            .ok_or_else(|| format!("{step_id}: no 'propose' step output available"))?;
        serde_json::from_value(value).map_err(|err| format!("{step_id}: failed to read proposed change: {err}"))
    }

    async fn execute_propose(&self, step: &StepSpec, task: &str, files: &[PathBuf], context: &mut ExecutionContext) -> StepOutcome {
        match self.engine.propose_diff(task, files).await {
            Ok(change) => {
                let file_count = change.files.len();
                context.outputs.insert(step.id.clone(), serde_json::to_value(&change).unwrap_or_default());
                StepOutcome { step_id: step.id.clone(), success: true, detail: format!("proposed changes to {file_count} file(s)") }
            }
            Err(err) => StepOutcome { step_id: step.id.clone(), success: false, detail: err.to_string() },
        }
    }

    async fn execute_ground(&self, step: &StepSpec, context: &mut ExecutionContext) -> StepOutcome {
        let change = match Self::proposed_change_from(context, &step.id) {
            Ok(change) => change,
            Err(detail) => return StepOutcome { step_id: step.id.clone(), success: false, detail },
        };

        let gate = pro_grounding::GroundingGate::new(self.repo_map);
        let mut all_findings = Vec::new();
        for file in &change.files {
            let lsp_arg = self.lsp_arg_for(&file.path);
            let report = gate.check(&file.path, &self.repo_root, &file.new_content, lsp_arg).await;
            all_findings.extend(report.findings);
        }

        let success = all_findings.is_empty();
        let finding_count = all_findings.len();
        context.outputs.insert(step.id.clone(), serde_json::json!({ "findings": all_findings }));
        StepOutcome { step_id: step.id.clone(), success, detail: format!("{finding_count} grounding finding(s)") }
    }

    fn lsp_arg_for(&self, file_path: &std::path::Path) -> Option<(&pro_lsp::LspClient, lsp_types::Uri)> {
        let client = self.lsp?;
        if pro_repomap::Language::of_path(file_path) != Some(pro_repomap::Language::Rust) {
            return None;
        }
        let absolute = self.repo_root.join(file_path);
        format!("file://{}", absolute.display()).parse::<lsp_types::Uri>().ok().map(|uri| (client, uri))
    }

    async fn execute_verify(&self, step: &StepSpec, command: &str, context: &mut ExecutionContext) -> StepOutcome {
        let change = match Self::proposed_change_from(context, &step.id) {
            Ok(change) => change,
            Err(detail) => return StepOutcome { step_id: step.id.clone(), success: false, detail },
        };

        for file in &change.files {
            let absolute = self.repo_root.join(&file.path);
            if let Err(err) = self.engine.fs_write(&absolute, file.new_content.as_bytes()) {
                return StepOutcome {
                    step_id: step.id.clone(),
                    success: false,
                    detail: format!("fs_write denied for {}: {err}", absolute.display()),
                };
            }
        }

        // Tier 0 (shadow) is the honest default here: no OS-level sandbox
        // backend exists yet (`pro-sandbox`), so this deliberately routes
        // to the observe-only enforcer rather than actually running the
        // verify command — see `AGENTS.md` for the same gap documented
        // against Phase 0's `run` path. A caller with a real backend
        // registered and an appropriate `Run` grant gets real execution;
        // this method doesn't hardcode a tier ceiling beyond what
        // `Engine::run` itself already enforces.
        let spec = pro_sandbox::RunSpec {
            argv: vec!["/bin/sh".to_string(), "-c".to_string(), command.to_string()],
            read_paths: vec![self.repo_root.clone()],
            write_paths: vec![self.repo_root.clone()],
            env_vars: Vec::new(),
            net_hosts: Vec::new(),
        };
        match self.engine.run(spec, 0) {
            Ok(outcome) => {
                StepOutcome { step_id: step.id.clone(), success: true, detail: format!("verify routed to enforcer: {outcome:?}") }
            }
            Err(err) => StepOutcome { step_id: step.id.clone(), success: false, detail: err.to_string() },
        }
    }

    async fn execute_review(&self, step: &StepSpec, mode: ReviewModeSpec, context: &mut ExecutionContext) -> StepOutcome {
        let change = match Self::proposed_change_from(context, &step.id) {
            Ok(change) => change,
            Err(detail) => return StepOutcome { step_id: step.id.clone(), success: false, detail },
        };
        let combined_diff = change.files.iter().map(|f| f.diff.as_str()).collect::<Vec<_>>().join("\n");

        let outcome = match mode {
            ReviewModeSpec::Simple => {
                pro_review::simple(self.engine.provider.as_ref(), &combined_diff).await.map(serde_json::to_value)
            }
            ReviewModeSpec::Adversarial => {
                pro_review::adversarial(self.engine.provider.as_ref(), &combined_diff).await.map(serde_json::to_value)
            }
            ReviewModeSpec::Orthogonal => {
                let Some(secondary) = &self.review_secondary else {
                    return StepOutcome {
                        step_id: step.id.clone(),
                        success: false,
                        detail: "orthogonal review requires a secondary provider (WorkflowRunner::with_review_secondary)".into(),
                    };
                };
                pro_review::orthogonal(self.engine.provider.as_ref(), secondary.as_ref(), &combined_diff)
                    .await
                    .map(serde_json::to_value)
            }
        };

        match outcome {
            Ok(Ok(value)) => {
                context.outputs.insert(step.id.clone(), value);
                StepOutcome { step_id: step.id.clone(), success: true, detail: "review complete".into() }
            }
            Ok(Err(err)) => {
                StepOutcome { step_id: step.id.clone(), success: false, detail: format!("failed to serialize review result: {err}") }
            }
            Err(err) => StepOutcome { step_id: step.id.clone(), success: false, detail: err.to_string() },
        }
    }
}

#[async_trait]
impl<'a> StepExecutor for WorkflowRunner<'a> {
    async fn execute(&mut self, step: &StepSpec, context: &mut ExecutionContext) -> StepOutcome {
        match &step.kind {
            StepKind::Propose { task, files } => self.execute_propose(step, task, files, context).await,
            StepKind::Ground => self.execute_ground(step, context).await,
            StepKind::Verify { command } => self.execute_verify(step, command, context).await,
            StepKind::Review { mode } => self.execute_review(step, *mode, context).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::propose::ProposedFile;
    use pro_audit::AuditLog;
    use pro_capability::{Capability, Grant, GrantStore};
    use pro_orchestrate::{run_workflow, StepKind, StepSpec, WorkflowSpec};
    use pro_provider::{CompletionRequest, CompletionResponse, ProviderError};
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

    fn scratch_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pro-core-workflow-test-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn temp_audit_path(name: &str) -> PathBuf {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("pro-core-workflow-test-{name}-{nonce}.jsonl"))
    }

    fn test_config() -> crate::Config {
        crate::Config {
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
    async fn propose_then_ground_flags_a_hallucinated_symbol_reference() {
        let dir = scratch_dir("ground-flag");
        let file_path = dir.join("lib.rs");
        std::fs::write(&file_path, "fn real_helper() {}\n").unwrap();
        let repo_map = pro_repomap::RepoMap::build(&dir);

        let mut grants = GrantStore::new();
        grants.add_grant(Grant::new_shadow(Capability::FsRead { paths: vec![dir.clone()] }));
        let reply = serde_json::json!({
            "files": [{ "path": file_path.to_string_lossy(), "new_content": "fn f() { totally_made_up_fn(); }\n" }]
        })
        .to_string();
        let provider = ScriptedProvider { reply };

        let audit_path = temp_audit_path("ground-flag");
        let engine =
            Engine::new(test_config(), grants, AuditLog::new(audit_path.clone()), EnforcerRegistry::new(), Box::new(provider));
        let mut runner = WorkflowRunner::new(&engine, &repo_map, dir.clone());

        let spec = WorkflowSpec {
            name: "test".into(),
            steps: vec![
                StepSpec {
                    id: "propose".into(),
                    kind: StepKind::Propose { task: "add a function".into(), files: vec![file_path.clone()] },
                    depends_on: vec![],
                },
                StepSpec { id: "ground".into(), kind: StepKind::Ground, depends_on: vec!["propose".into()] },
            ],
        };

        let outcomes = run_workflow(&spec, &mut runner).await.unwrap();
        assert_eq!(outcomes.len(), 2);
        assert!(outcomes[0].success, "propose should succeed: {}", outcomes[0].detail);
        assert!(!outcomes[1].success, "ground should flag the hallucinated symbol");

        std::fs::remove_dir_all(&dir).ok();
        std::fs::remove_file(&audit_path).ok();
    }

    #[tokio::test]
    async fn verify_step_is_denied_without_a_run_grant() {
        let dir = scratch_dir("verify-denied");
        std::fs::write(dir.join("lib.rs"), "fn a() {}\n").unwrap();
        let repo_map = pro_repomap::RepoMap::build(&dir);

        let provider = ScriptedProvider { reply: "{}".into() };
        let audit_path = temp_audit_path("verify-denied");
        let engine = Engine::new(
            test_config(),
            GrantStore::new(),
            AuditLog::new(audit_path.clone()),
            EnforcerRegistry::new(),
            Box::new(provider),
        );
        let mut runner = WorkflowRunner::new(&engine, &repo_map, dir.clone());

        let mut context = ExecutionContext::default();
        let change = ProposedChange {
            files: vec![ProposedFile {
                path: dir.join("lib.rs"),
                old_content: Some("fn a() {}\n".into()),
                new_content: "fn a() {}\n".into(),
                diff: String::new(),
            }],
        };
        context.outputs.insert("propose".into(), serde_json::to_value(&change).unwrap());

        let step = StepSpec { id: "verify".into(), kind: StepKind::Verify { command: "true".into() }, depends_on: vec![] };
        let outcome = runner.execute(&step, &mut context).await;
        assert!(!outcome.success);

        std::fs::remove_dir_all(&dir).ok();
        std::fs::remove_file(&audit_path).ok();
    }
}
