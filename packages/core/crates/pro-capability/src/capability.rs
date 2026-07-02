use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// A single argv-shape validator for a `run` grant: the binary path plus a
/// rule for which arguments are acceptable. Deliberately not a regex over
/// the raw command string (`02-security-core.md` §1) — arguments that look
/// like filesystem paths are checked against the read/write scopes already
/// granted, everything else just has to match one of the declared literals.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunBinary {
    pub path: PathBuf,
    pub allowed_literal_args: Vec<String>,
}

impl RunBinary {
    /// Validates argv against this binary's shape: argv[0] must match
    /// `path`, and every subsequent argument must either be an allowed
    /// literal or a path that falls within `read_paths`/`write_paths`.
    pub fn validate_argv(&self, argv: &[String], read_paths: &[PathBuf], write_paths: &[PathBuf]) -> bool {
        let Some(first) = argv.first() else {
            return false;
        };
        if Path::new(first) != self.path {
            return false;
        }
        argv.iter().skip(1).all(|arg| {
            if self.allowed_literal_args.iter().any(|lit| lit == arg) {
                return true;
            }
            let candidate = Path::new(arg);
            if !candidate.is_absolute() {
                return false;
            }
            path_within_any(candidate, read_paths) || path_within_any(candidate, write_paths)
        })
    }
}

fn path_within_any(candidate: &Path, scopes: &[PathBuf]) -> bool {
    scopes.iter().any(|scope| candidate.starts_with(scope))
}

/// A scoped capability grant. Every variant carries an explicit scope —
/// never a bare boolean — per the policy-layer requirement in
/// `02-security-core.md` §1.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Capability {
    FsRead { paths: Vec<PathBuf> },
    FsWrite { paths: Vec<PathBuf> },
    Net { hosts: Vec<String> },
    Run { binaries: Vec<RunBinary> },
    Env { vars: Vec<String> },
}

impl Capability {
    pub fn kind(&self) -> &'static str {
        match self {
            Capability::FsRead { .. } => "fs_read",
            Capability::FsWrite { .. } => "fs_write",
            Capability::Net { .. } => "net",
            Capability::Run { .. } => "run",
            Capability::Env { .. } => "env",
        }
    }

    /// Whether `self` (a granted capability) covers `requested` (a
    /// requested action). `requested`'s scope must be a subset of `self`'s.
    pub fn covers(&self, requested: &Capability) -> bool {
        match (self, requested) {
            (Capability::FsRead { paths: granted }, Capability::FsRead { paths: wanted }) => {
                wanted.iter().all(|p| path_within_any(p, granted))
            }
            (Capability::FsWrite { paths: granted }, Capability::FsWrite { paths: wanted }) => {
                wanted.iter().all(|p| path_within_any(p, granted))
            }
            (Capability::Net { hosts: granted }, Capability::Net { hosts: wanted }) => {
                wanted.iter().all(|h| granted.iter().any(|g| g == "*" || g == h))
            }
            (Capability::Run { binaries: granted }, Capability::Run { binaries: wanted }) => {
                wanted.iter().all(|w| granted.iter().any(|g| g.path == w.path))
            }
            (Capability::Env { vars: granted }, Capability::Env { vars: wanted }) => {
                wanted.iter().all(|v| granted.contains(v))
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fs_read_covers_subset_path() {
        let granted = Capability::FsRead { paths: vec![PathBuf::from("/home/user/project")] };
        let wanted = Capability::FsRead { paths: vec![PathBuf::from("/home/user/project/src/main.rs")] };
        assert!(granted.covers(&wanted));
    }

    #[test]
    fn fs_read_rejects_path_outside_scope() {
        let granted = Capability::FsRead { paths: vec![PathBuf::from("/home/user/project")] };
        let wanted = Capability::FsRead { paths: vec![PathBuf::from("/etc/passwd")] };
        assert!(!granted.covers(&wanted));
    }

    #[test]
    fn net_wildcard_covers_any_host() {
        let granted = Capability::Net { hosts: vec!["*".into()] };
        let wanted = Capability::Net { hosts: vec!["evil.example".into()] };
        assert!(granted.covers(&wanted));
    }

    #[test]
    fn net_scoped_rejects_unlisted_host() {
        let granted = Capability::Net { hosts: vec!["api.github.com".into()] };
        let wanted = Capability::Net { hosts: vec!["evil.example".into()] };
        assert!(!granted.covers(&wanted));
    }

    #[test]
    fn run_binary_validates_argv_against_fs_scopes() {
        let bin = RunBinary { path: PathBuf::from("/usr/bin/cat"), allowed_literal_args: vec![] };
        let read_paths = vec![PathBuf::from("/home/user/project")];
        assert!(bin.validate_argv(
            &["/usr/bin/cat".into(), "/home/user/project/README.md".into()],
            &read_paths,
            &[],
        ));
        assert!(!bin.validate_argv(
            &["/usr/bin/cat".into(), "/etc/shadow".into()],
            &read_paths,
            &[],
        ));
    }
}
