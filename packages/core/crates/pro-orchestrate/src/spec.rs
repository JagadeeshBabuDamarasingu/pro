use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReviewModeSpec {
    Simple,
    Adversarial,
    Orthogonal,
}

/// The fixed step vocabulary for this pass (not a generic plugin system —
/// see `01-architecture.md`'s "control flow should be inspectable and
/// reproducible, not hidden inside prompts"). Each variant is executed by
/// whatever `StepExecutor` the caller supplies (`pro-core`'s
/// `WorkflowRunner`, in practice) — this crate owns the vocabulary and the
/// dependency graph, not the side effects.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StepKind {
    Propose { task: String, files: Vec<PathBuf> },
    Ground,
    Verify { command: String },
    Review { mode: ReviewModeSpec },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepSpec {
    pub id: String,
    #[serde(flatten)]
    pub kind: StepKind,
    #[serde(default)]
    pub depends_on: Vec<String>,
}

/// A declarative workflow — loadable from YAML (`.pro/workflows/*.yaml`)
/// so the codegen pipeline is inspectable/reproducible, not just a Rust
/// function. `codegen_pipeline` below builds the one fixed pipeline this
/// pass actually needs; arbitrary custom graphs are already representable
/// in this same struct for later phases.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowSpec {
    pub name: String,
    pub steps: Vec<StepSpec>,
}

impl WorkflowSpec {
    /// The Propose → Ground → Verify → Review pipeline this phase exists
    /// to run.
    pub fn codegen_pipeline(
        task: impl Into<String>,
        files: Vec<PathBuf>,
        verify_command: impl Into<String>,
        review_mode: ReviewModeSpec,
    ) -> WorkflowSpec {
        WorkflowSpec {
            name: "codegen".to_string(),
            steps: vec![
                StepSpec {
                    id: "propose".to_string(),
                    kind: StepKind::Propose { task: task.into(), files },
                    depends_on: Vec::new(),
                },
                StepSpec {
                    id: "ground".to_string(),
                    kind: StepKind::Ground,
                    depends_on: vec!["propose".to_string()],
                },
                StepSpec {
                    id: "verify".to_string(),
                    kind: StepKind::Verify { command: verify_command.into() },
                    depends_on: vec!["ground".to_string()],
                },
                StepSpec {
                    id: "review".to_string(),
                    kind: StepKind::Review { mode: review_mode },
                    depends_on: vec!["verify".to_string()],
                },
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codegen_pipeline_round_trips_through_yaml() {
        let spec = WorkflowSpec::codegen_pipeline(
            "add a test",
            vec![PathBuf::from("src/lib.rs")],
            "cargo test",
            ReviewModeSpec::Adversarial,
        );
        let yaml = serde_yaml::to_string(&spec).unwrap();
        let parsed: WorkflowSpec = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(parsed.steps.len(), 4);
        assert_eq!(parsed.steps[0].id, "propose");
        assert!(matches!(parsed.steps[3].kind, StepKind::Review { mode: ReviewModeSpec::Adversarial }));
    }
}
