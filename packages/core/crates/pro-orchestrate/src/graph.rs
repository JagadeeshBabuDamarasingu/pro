use std::collections::HashMap;
use std::path::Path;

use petgraph::graph::{DiGraph, NodeIndex};

use crate::spec::WorkflowSpec;

#[derive(Debug, thiserror::Error)]
pub enum OrchestrateError {
    #[error("step '{0}' depends on unknown step '{1}'")]
    UnknownDependency(String, String),
    #[error("workflow graph has a dependency cycle")]
    Cycle,
    #[error("failed to read workflow file: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to parse workflow YAML: {0}")]
    Yaml(#[from] serde_yaml::Error),
}

/// Returns step ids in an order that respects `depends_on`. Sequential
/// execution for this pass — the graph gives inspectable, reproducible
/// ordering (`01-architecture.md`); true intra-graph parallel execution is
/// a documented future enhancement, not needed yet since the worktree/
/// best-of-N mechanism is this phase's actual parallelism.
pub fn topological_order(spec: &WorkflowSpec) -> Result<Vec<String>, OrchestrateError> {
    let mut graph: DiGraph<&str, ()> = DiGraph::new();
    let mut index_of: HashMap<&str, NodeIndex> = HashMap::new();
    for step in &spec.steps {
        index_of.insert(step.id.as_str(), graph.add_node(step.id.as_str()));
    }
    for step in &spec.steps {
        let &to = index_of.get(step.id.as_str()).expect("just inserted");
        for dep in &step.depends_on {
            let &from = index_of
                .get(dep.as_str())
                .ok_or_else(|| OrchestrateError::UnknownDependency(step.id.clone(), dep.clone()))?;
            graph.add_edge(from, to, ());
        }
    }
    petgraph::algo::toposort(&graph, None)
        .map(|order| order.into_iter().map(|idx| graph[idx].to_string()).collect())
        .map_err(|_| OrchestrateError::Cycle)
}

pub fn load_yaml(path: &Path) -> Result<WorkflowSpec, OrchestrateError> {
    let raw = std::fs::read_to_string(path)?;
    Ok(serde_yaml::from_str(&raw)?)
}

pub fn to_yaml(spec: &WorkflowSpec) -> Result<String, OrchestrateError> {
    Ok(serde_yaml::to_string(spec)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::{ReviewModeSpec, StepSpec};

    #[test]
    fn codegen_pipeline_orders_propose_before_review() {
        let spec = WorkflowSpec::codegen_pipeline("task", vec![], "cargo test", ReviewModeSpec::Simple);
        let order = topological_order(&spec).unwrap();
        assert_eq!(order, vec!["propose", "ground", "verify", "review"]);
    }

    #[test]
    fn cycle_is_detected() {
        let spec = WorkflowSpec {
            name: "cyclic".into(),
            steps: vec![
                StepSpec { id: "a".into(), kind: crate::spec::StepKind::Ground, depends_on: vec!["b".into()] },
                StepSpec { id: "b".into(), kind: crate::spec::StepKind::Ground, depends_on: vec!["a".into()] },
            ],
        };
        assert!(matches!(topological_order(&spec), Err(OrchestrateError::Cycle)));
    }

    #[test]
    fn unknown_dependency_is_reported() {
        let spec = WorkflowSpec {
            name: "broken".into(),
            steps: vec![StepSpec {
                id: "a".into(),
                kind: crate::spec::StepKind::Ground,
                depends_on: vec!["does-not-exist".into()],
            }],
        };
        assert!(matches!(topological_order(&spec), Err(OrchestrateError::UnknownDependency(_, _))));
    }
}
