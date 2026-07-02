use std::collections::HashMap;

use async_trait::async_trait;

use crate::graph::{topological_order, OrchestrateError};
use crate::spec::{StepSpec, WorkflowSpec};

/// Accumulates each step's output for later steps to read (e.g. `Ground`
/// needs the file content `Propose` produced).
#[derive(Debug, Default)]
pub struct ExecutionContext {
    pub outputs: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct StepOutcome {
    pub step_id: String,
    pub success: bool,
    pub detail: String,
}

/// Implemented by the caller that has access to the actual side-effecting
/// machinery (`pro-core`'s `WorkflowRunner`, composing `pro-repomap` /
/// `pro-grounding` / `pro-worktree` / `pro-review` / the provider) — this
/// crate owns the step vocabulary and execution order, not the
/// capability-gated side effects themselves.
#[async_trait]
pub trait StepExecutor: Send + Sync {
    async fn execute(&mut self, step: &StepSpec, context: &mut ExecutionContext) -> StepOutcome;
}

/// Runs every step in topological order, fail-fast: a failed step stops
/// the workflow rather than running `Verify` after a failed `Ground`, etc.
pub async fn run_workflow(
    spec: &WorkflowSpec,
    executor: &mut dyn StepExecutor,
) -> Result<Vec<StepOutcome>, OrchestrateError> {
    let order = topological_order(spec)?;
    let mut context = ExecutionContext::default();
    let mut outcomes = Vec::with_capacity(order.len());

    for step_id in order {
        let step = spec.steps.iter().find(|s| s.id == step_id).expect("topological order only contains known ids");
        let outcome = executor.execute(step, &mut context).await;
        let should_stop = !outcome.success;
        outcomes.push(outcome);
        if should_stop {
            break;
        }
    }
    Ok(outcomes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::ReviewModeSpec;

    struct RecordingExecutor {
        executed: Vec<String>,
        fail_at: Option<&'static str>,
    }

    #[async_trait]
    impl StepExecutor for RecordingExecutor {
        async fn execute(&mut self, step: &StepSpec, _context: &mut ExecutionContext) -> StepOutcome {
            self.executed.push(step.id.clone());
            let success = self.fail_at != Some(step.id.as_str());
            StepOutcome { step_id: step.id.clone(), success, detail: "ok".into() }
        }
    }

    #[tokio::test]
    async fn runs_every_step_in_dependency_order_on_success() {
        let spec = WorkflowSpec::codegen_pipeline("task", vec![], "cargo test", ReviewModeSpec::Simple);
        let mut executor = RecordingExecutor { executed: Vec::new(), fail_at: None };
        let outcomes = run_workflow(&spec, &mut executor).await.unwrap();

        assert_eq!(executor.executed, vec!["propose", "ground", "verify", "review"]);
        assert!(outcomes.iter().all(|o| o.success));
    }

    #[tokio::test]
    async fn stops_after_first_failing_step() {
        let spec = WorkflowSpec::codegen_pipeline("task", vec![], "cargo test", ReviewModeSpec::Simple);
        let mut executor = RecordingExecutor { executed: Vec::new(), fail_at: Some("ground") };
        let outcomes = run_workflow(&spec, &mut executor).await.unwrap();

        assert_eq!(executor.executed, vec!["propose", "ground"]);
        assert_eq!(outcomes.len(), 2);
        assert!(!outcomes[1].success);
    }
}
