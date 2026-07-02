use std::path::Path;
use std::process::Command;

use async_trait::async_trait;

use crate::git::{add, remove, Worktree, WorktreeError};

/// Implemented by whatever produces a candidate's changes inside its own
/// worktree (in practice, `pro-orchestrate`'s `Propose` step). Kept as a
/// trait rather than a generic async closure — matches the existing
/// `Provider`/`ClusteringStrategy` pattern elsewhere in this workspace for
/// async trait objects.
#[async_trait]
pub trait Proposer: Send + Sync {
    async fn propose(&self, worktree: &Worktree, attempt: usize);
}

#[derive(Debug, Clone)]
pub struct CandidateResult {
    pub worktree: Worktree,
    pub verify_passed: bool,
    pub verify_output: String,
}

#[derive(Debug, Clone)]
pub struct BestOfNResult {
    pub candidates: Vec<CandidateResult>,
    pub winner_index: Option<usize>,
}

fn run_verify(worktree: &Worktree, verify_cmd: &str) -> (bool, String) {
    match Command::new("sh").arg("-c").arg(verify_cmd).current_dir(&worktree.path).output() {
        Ok(output) => {
            let mut combined = String::from_utf8_lossy(&output.stdout).to_string();
            combined.push_str(&String::from_utf8_lossy(&output.stderr));
            (output.status.success(), combined)
        }
        Err(err) => (false, format!("failed to run verify command: {err}")),
    }
}

/// Spins up `n` linked worktrees off `repo_root`, runs the same task in
/// each via `proposer` (varied temperature/seed is the proposer's
/// responsibility — this orchestrator just gives each attempt its own
/// isolated directory), scores by `verify_cmd`'s exit code — a real
/// verifier, not an LLM-judge — and tears down every candidate except the
/// first that passes. `03-audience-developers-builders.md`'s
/// `pro run --parallel N --verify <test-cmd>` primitive.
pub async fn run(
    repo_root: &Path,
    worktree_base_dir: &Path,
    n: usize,
    verify_cmd: &str,
    proposer: &dyn Proposer,
) -> Result<BestOfNResult, WorktreeError> {
    let batch_id = uuid::Uuid::new_v4().simple().to_string();
    let mut candidates = Vec::with_capacity(n);

    for attempt in 0..n {
        let branch = format!("pro/best-of-n-{batch_id}-{attempt}");
        let worktree_dir = worktree_base_dir.join(format!("best-of-n-{batch_id}-{attempt}"));
        let worktree = add(repo_root, &worktree_dir, &branch)?;

        proposer.propose(&worktree, attempt).await;

        let (verify_passed, verify_output) = run_verify(&worktree, verify_cmd);
        candidates.push(CandidateResult { worktree, verify_passed, verify_output });
    }

    let winner_index = candidates.iter().position(|c| c.verify_passed);

    for (index, candidate) in candidates.iter().enumerate() {
        if Some(index) != winner_index {
            let _ = remove(repo_root, &candidate.worktree);
        }
    }

    Ok(BestOfNResult { candidates, winner_index })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command as StdCommand;

    fn init_scratch_repo(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("pro-worktree-bestofn-test-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let run = |args: &[&str]| {
            let status = StdCommand::new("git").arg("-C").arg(&dir).args(args).status().unwrap();
            assert!(status.success());
        };
        run(&["init", "-q"]);
        run(&["config", "user.email", "test@example.com"]);
        run(&["config", "user.name", "Test"]);
        std::fs::write(dir.join("README.md"), "scratch\n").unwrap();
        run(&["add", "."]);
        run(&["commit", "-q", "-m", "initial"]);
        dir
    }

    struct WriteMarkerFile {
        pass_on_attempt: usize,
    }

    #[async_trait]
    impl Proposer for WriteMarkerFile {
        async fn propose(&self, worktree: &Worktree, attempt: usize) {
            let marker = if attempt == self.pass_on_attempt { "pass" } else { "fail" };
            std::fs::write(worktree.path.join("marker.txt"), marker).unwrap();
        }
    }

    #[tokio::test]
    async fn only_the_verifier_passing_candidate_survives() {
        let repo = init_scratch_repo("survive");
        let worktree_base = repo.parent().unwrap().to_path_buf();
        let proposer = WriteMarkerFile { pass_on_attempt: 1 };

        let result = run(
            &repo,
            &worktree_base,
            3,
            "grep -q pass marker.txt",
            &proposer,
        )
        .await
        .unwrap();

        assert_eq!(result.winner_index, Some(1));
        assert!(result.candidates[1].worktree.path.exists(), "winner worktree should survive");
        assert!(!result.candidates[0].worktree.path.exists(), "loser worktree should be torn down");
        assert!(!result.candidates[2].worktree.path.exists(), "loser worktree should be torn down");

        std::fs::remove_dir_all(&result.candidates[1].worktree.path).ok();
        std::fs::remove_dir_all(&repo).ok();
    }

    #[tokio::test]
    async fn no_passing_candidate_tears_down_all_and_reports_no_winner() {
        let repo = init_scratch_repo("nowinner");
        let worktree_base = repo.parent().unwrap().to_path_buf();
        let proposer = WriteMarkerFile { pass_on_attempt: usize::MAX };

        let result = run(&repo, &worktree_base, 2, "grep -q pass marker.txt", &proposer).await.unwrap();

        assert_eq!(result.winner_index, None);
        for candidate in &result.candidates {
            assert!(!candidate.worktree.path.exists());
        }

        std::fs::remove_dir_all(&repo).ok();
    }
}
