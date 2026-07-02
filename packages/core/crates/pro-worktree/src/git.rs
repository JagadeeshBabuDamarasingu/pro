use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, thiserror::Error)]
pub enum WorktreeError {
    #[error("failed to run git: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("git worktree {op} failed: {stderr}")]
    GitFailed { op: &'static str, stderr: String },
}

#[derive(Debug, Clone)]
pub struct Worktree {
    pub path: PathBuf,
    pub branch: String,
}

fn run_git(repo_root: &Path, args: &[&str], op: &'static str) -> Result<(), WorktreeError> {
    let output = Command::new("git").arg("-C").arg(repo_root).args(args).output()?;
    if !output.status.success() {
        return Err(WorktreeError::GitFailed { op, stderr: String::from_utf8_lossy(&output.stderr).to_string() });
    }
    Ok(())
}

/// Creates a new linked worktree on a fresh branch off the repo's current
/// `HEAD` — the isolated execution context each best-of-N candidate (and,
/// per the spec, each cloud task session and each OSS-maintainer external-
/// contribution sandbox) runs inside.
pub fn add(repo_root: &Path, worktree_dir: &Path, branch: &str) -> Result<Worktree, WorktreeError> {
    let dir_str = worktree_dir.to_string_lossy().to_string();
    run_git(repo_root, &["worktree", "add", "-b", branch, &dir_str], "add")?;
    Ok(Worktree { path: worktree_dir.to_path_buf(), branch: branch.to_string() })
}

/// Tears down a worktree and deletes its branch — real cleanup, not just
/// removing the directory, so a losing best-of-N candidate doesn't leave a
/// dangling branch behind.
pub fn remove(repo_root: &Path, worktree: &Worktree) -> Result<(), WorktreeError> {
    let dir_str = worktree.path.to_string_lossy().to_string();
    run_git(repo_root, &["worktree", "remove", "--force", &dir_str], "remove")?;
    // Branch deletion is best-effort: if it fails (e.g. already gone), the
    // worktree itself is still torn down, which is the part that matters.
    let _ = run_git(repo_root, &["branch", "-D", &worktree.branch], "branch -D");
    Ok(())
}

pub fn list(repo_root: &Path) -> Result<Vec<Worktree>, WorktreeError> {
    let output = Command::new("git").arg("-C").arg(repo_root).args(["worktree", "list", "--porcelain"]).output()?;
    if !output.status.success() {
        return Err(WorktreeError::GitFailed { op: "list", stderr: String::from_utf8_lossy(&output.stderr).to_string() });
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut worktrees = Vec::new();
    let mut current_path: Option<PathBuf> = None;
    for line in text.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            current_path = Some(PathBuf::from(path));
        } else if let Some(branch_ref) = line.strip_prefix("branch ") {
            if let Some(path) = current_path.take() {
                let branch = branch_ref.strip_prefix("refs/heads/").unwrap_or(branch_ref).to_string();
                worktrees.push(Worktree { path, branch });
            }
        }
    }
    Ok(worktrees)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init_scratch_repo(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pro-worktree-test-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let run = |args: &[&str]| {
            let status = Command::new("git").arg("-C").arg(&dir).args(args).status().unwrap();
            assert!(status.success(), "git {args:?} failed");
        };
        run(&["init", "-q"]);
        run(&["config", "user.email", "test@example.com"]);
        run(&["config", "user.name", "Test"]);
        std::fs::write(dir.join("README.md"), "scratch repo\n").unwrap();
        run(&["add", "."]);
        run(&["commit", "-q", "-m", "initial"]);
        dir
    }

    #[test]
    fn add_list_remove_round_trip() {
        let repo = init_scratch_repo("roundtrip");
        let worktree_dir = repo.parent().unwrap().join(format!("pro-worktree-test-wt-{}", std::process::id()));

        let worktree = add(&repo, &worktree_dir, "pro/test-branch").unwrap();
        assert!(worktree.path.exists());

        let listed = list(&repo).unwrap();
        assert!(listed.iter().any(|w| w.branch == "pro/test-branch"));

        remove(&repo, &worktree).unwrap();
        assert!(!worktree.path.exists());

        std::fs::remove_dir_all(&repo).ok();
    }
}
