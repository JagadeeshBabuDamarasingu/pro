use std::path::{Path, PathBuf};

use crate::capability::{Capability, RunBinary};

/// Default profiles (mirroring Codex CLI's `read-only`/`workspace`/
/// `danger-full-access`) so users aren't forced to hand-author policy.
/// Leaving `workspace` always requires an explicit opt-in step — this
/// module only builds the capability *list*; nothing here auto-elevates a
/// grant's autonomy tier past `Shadow`.
pub fn read_only(project_root: &Path) -> Vec<Capability> {
    vec![Capability::FsRead { paths: vec![project_root.to_path_buf()] }]
}

pub fn workspace(project_root: &Path) -> Vec<Capability> {
    let mut caps = read_only(project_root);
    caps.push(Capability::FsWrite { paths: vec![project_root.to_path_buf()] });
    caps
}

pub fn danger_full_access() -> Vec<Capability> {
    vec![
        Capability::FsRead { paths: vec![PathBuf::from("/")] },
        Capability::FsWrite { paths: vec![PathBuf::from("/")] },
        Capability::Net { hosts: vec!["*".into()] },
        Capability::Run {
            binaries: vec![RunBinary { path: PathBuf::from("/bin/sh"), allowed_literal_args: vec!["-c".into()] }],
        },
        Capability::Env { vars: vec!["*".into()] },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_only_has_no_write_capability() {
        let caps = read_only(Path::new("/project"));
        assert!(!caps.iter().any(|c| matches!(c, Capability::FsWrite { .. })));
    }

    #[test]
    fn workspace_adds_write_on_top_of_read_only() {
        let caps = workspace(Path::new("/project"));
        assert!(caps.iter().any(|c| matches!(c, Capability::FsRead { .. })));
        assert!(caps.iter().any(|c| matches!(c, Capability::FsWrite { .. })));
    }
}
