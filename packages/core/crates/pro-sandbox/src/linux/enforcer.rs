use std::path::{Path, PathBuf};
use std::process::Command;

use crate::linux::argv::{build_bwrap_argv, standard_host_binds};
use crate::{EnforcementOutcome, LandlockEnforcement, RunSpec, SandboxEnforcer, SandboxError};

/// Real Linux enforcement backend: bubblewrap for namespace/mount
/// isolation (the primary, always-active boundary), Landlock for
/// defense-in-depth path restriction applied *inside* the sandbox by
/// `pro-sandbox-helper` immediately before it execs the real target.
/// Landlock's status is honestly recorded on every `Executed` outcome,
/// never assumed enforced.
pub struct LinuxBwrapEnforcer {
    bwrap_path: PathBuf,
    helper_path: PathBuf,
}

impl LinuxBwrapEnforcer {
    /// Resolves `bwrap` via `PATH` and the helper binary via
    /// `current_exe().with_file_name("pro-sandbox-helper")`, verifying
    /// both are present *now* — hard error at construction, never
    /// deferred to first `enforce()`, matching
    /// `EnforcerRegistry::enforcer_for_tier`'s "hard-error, not
    /// silent-skip" shape.
    ///
    /// Known constraint (documented, not solved here): relies on
    /// `pro-sandbox-helper` sharing a target/output directory with the
    /// calling binary, true for `cargo build`/`cargo test`/`cargo run`.
    /// Whenever npm/Tauri packaging splits binaries into separate
    /// installed locations, this resolution strategy must change.
    pub fn new() -> Result<Self, SandboxError> {
        let helper_path = std::env::current_exe()
            .map_err(SandboxError::Spawn)?
            .with_file_name("pro-sandbox-helper");
        Self::with_paths(PathBuf::from("bwrap"), helper_path)
    }

    /// Explicit-path constructor, primarily for tests: point at an
    /// arbitrary `bwrap` and helper binary rather than relying on
    /// `PATH`/`current_exe()`.
    pub fn with_paths(bwrap_path: PathBuf, helper_path: PathBuf) -> Result<Self, SandboxError> {
        Self::verify_bwrap_present(&bwrap_path)?;
        Self::verify_helper_present(&helper_path)?;
        Ok(Self { bwrap_path, helper_path })
    }

    fn verify_bwrap_present(bwrap_path: &Path) -> Result<(), SandboxError> {
        match Command::new(bwrap_path).arg("--version").output() {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Err(SandboxError::BackendBinaryNotFound(bwrap_path.display().to_string()))
            }
            Err(e) => Err(SandboxError::Spawn(e)),
        }
    }

    fn verify_helper_present(helper_path: &Path) -> Result<(), SandboxError> {
        if helper_path.is_file() {
            Ok(())
        } else {
            Err(SandboxError::HelperBinaryNotFound(helper_path.to_path_buf()))
        }
    }
}

impl SandboxEnforcer for LinuxBwrapEnforcer {
    fn enforce(&self, spec: &RunSpec) -> Result<EnforcementOutcome, SandboxError> {
        if !spec.net_hosts.is_empty() {
            return Err(SandboxError::PerHostNetworkFilteringUnsupported {
                hosts: spec.net_hosts.clone(),
            });
        }

        let status_file =
            std::env::temp_dir().join(format!("pro-sandbox-status-{}.json", uuid::Uuid::new_v4()));
        std::fs::write(&status_file, b"")
            .map_err(|source| SandboxError::StatusFileIo { path: status_file.clone(), source })?;

        let resolved_env: Vec<(String, String)> = spec
            .env_vars
            .iter()
            .filter_map(|name| std::env::var(name).ok().map(|value| (name.clone(), value)))
            .collect();

        let host_binds = standard_host_binds();
        let argv =
            build_bwrap_argv(spec, &host_binds, &self.helper_path, &status_file, &resolved_env);

        let run_result = Command::new(&self.bwrap_path).args(&argv).output();
        let landlock = read_status_file(&status_file);
        let _ = std::fs::remove_file(&status_file);

        let output = run_result.map_err(SandboxError::Spawn)?;
        let landlock = landlock?;

        Ok(EnforcementOutcome::Executed {
            stdout: output.stdout,
            stderr: output.stderr,
            exit_code: output.status.code().unwrap_or(-1),
            landlock,
        })
    }
}

fn read_status_file(path: &Path) -> Result<LandlockEnforcement, SandboxError> {
    let bytes = std::fs::read(path)
        .map_err(|e| SandboxError::HelperStatusUnreadable(format!("{path:?}: {e}")))?;
    serde_json::from_slice(&bytes)
        .map_err(|e| SandboxError::HelperStatusUnreadable(format!("{path:?}: malformed JSON: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(argv: &[&str]) -> RunSpec {
        RunSpec {
            argv: argv.iter().map(|s| s.to_string()).collect(),
            read_paths: vec![],
            write_paths: vec![],
            env_vars: vec![],
            net_hosts: vec![],
        }
    }

    fn real_helper_path() -> PathBuf {
        // `CARGO_BIN_EXE_pro-sandbox-helper` is only set by Cargo for
        // integration tests/benches, not for a crate's own unit tests
        // (confirmed: `env!` fails to resolve it here at compile time) —
        // so resolve by hand. Unit test binaries live in
        // `target/<profile>/deps/`; the `pro-sandbox-helper` bin lands one
        // level up in `target/<profile>/`, the same layout
        // `LinuxBwrapEnforcer::new()` relies on for production use via
        // `current_exe().with_file_name(...)`.
        let exe = std::env::current_exe().expect("current_exe should resolve in a test binary");
        let sibling = exe.with_file_name("pro-sandbox-helper");
        if sibling.is_file() {
            return sibling;
        }
        exe.parent()
            .and_then(|p| p.parent())
            .map(|p| p.join("pro-sandbox-helper"))
            .filter(|p| p.is_file())
            .unwrap_or_else(|| {
                panic!(
                    "could not locate pro-sandbox-helper near {exe:?}; run `cargo build -p \
                     pro-sandbox --bin pro-sandbox-helper` first"
                )
            })
    }

    #[test]
    fn linux_bwrap_enforcer_new_succeeds_when_bwrap_and_helper_present() {
        let enforcer = LinuxBwrapEnforcer::with_paths(PathBuf::from("bwrap"), real_helper_path());
        assert!(enforcer.is_ok(), "expected Ok, got {:?}", enforcer.err());
    }

    #[test]
    fn linux_bwrap_enforcer_new_fails_with_backend_binary_not_found_for_nonexistent_bwrap() {
        let result = LinuxBwrapEnforcer::with_paths(
            PathBuf::from("/definitely/not/a/real/bwrap"),
            real_helper_path(),
        );
        assert!(matches!(result, Err(SandboxError::BackendBinaryNotFound(_))));
    }

    #[test]
    fn linux_bwrap_enforcer_new_fails_with_helper_binary_not_found_for_missing_helper() {
        let result = LinuxBwrapEnforcer::with_paths(
            PathBuf::from("bwrap"),
            PathBuf::from("/definitely/not/a/real/helper"),
        );
        assert!(matches!(result, Err(SandboxError::HelperBinaryNotFound(_))));
    }

    #[test]
    fn enforce_rejects_nonempty_net_hosts_before_spawning_anything() {
        // Not asserted via a temp-dir diff: other tests run concurrently
        // and create their own `pro-sandbox-status-*` files in the same
        // shared `/tmp`, making a before/after file count racy. The
        // source of truth that nothing gets spawned is structural instead
        // — `enforce()` returns this error as its very first statement,
        // before `std::env::temp_dir()` is ever touched (see
        // `LinuxBwrapEnforcer::enforce` above).
        let enforcer =
            LinuxBwrapEnforcer::with_paths(PathBuf::from("bwrap"), real_helper_path()).unwrap();
        let mut s = spec(&["/bin/echo", "hi"]);
        s.net_hosts = vec!["example.com".to_string()];

        let result = enforcer.enforce(&s);
        assert!(matches!(result, Err(SandboxError::PerHostNetworkFilteringUnsupported { .. })));
    }

    #[test]
    fn enforce_runs_real_echo_inside_bwrap_and_captures_stdout() {
        let enforcer =
            LinuxBwrapEnforcer::with_paths(PathBuf::from("bwrap"), real_helper_path()).unwrap();
        let outcome = enforcer.enforce(&spec(&["/bin/echo", "hello-from-sandbox"])).unwrap();
        match outcome {
            EnforcementOutcome::Executed { stdout, exit_code, .. } => {
                assert_eq!(exit_code, 0);
                assert!(String::from_utf8_lossy(&stdout).contains("hello-from-sandbox"));
            }
            other => panic!("expected Executed, got {other:?}"),
        }
    }

    #[test]
    fn enforce_denies_read_outside_granted_paths() {
        let enforcer =
            LinuxBwrapEnforcer::with_paths(PathBuf::from("bwrap"), real_helper_path()).unwrap();
        // /etc/hostname exists on the host but is never bound into the
        // sandbox (not in standard_host_binds, not granted) -> namespace
        // makes it genuinely absent, not merely permission-denied.
        let outcome =
            enforcer.enforce(&spec(&["/bin/cat", "/etc/hostname"])).unwrap();
        match outcome {
            EnforcementOutcome::Executed { exit_code, stderr, .. } => {
                assert_ne!(exit_code, 0);
                let stderr = String::from_utf8_lossy(&stderr);
                assert!(
                    stderr.contains("No such file or directory") || stderr.contains("cannot open"),
                    "expected a namespace-level absence error, got: {stderr}"
                );
            }
            other => panic!("expected Executed, got {other:?}"),
        }
    }

    #[test]
    fn enforce_allows_read_within_granted_read_path() {
        let dir = std::env::temp_dir().join(format!("pro-sandbox-test-read-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("in.txt");
        std::fs::write(&file, b"granted-content").unwrap();

        let enforcer =
            LinuxBwrapEnforcer::with_paths(PathBuf::from("bwrap"), real_helper_path()).unwrap();
        let mut s = spec(&["/bin/cat", file.to_str().unwrap()]);
        s.read_paths = vec![dir.clone()];

        let outcome = enforcer.enforce(&s).unwrap();
        std::fs::remove_dir_all(&dir).ok();
        match outcome {
            EnforcementOutcome::Executed { stdout, exit_code, .. } => {
                assert_eq!(exit_code, 0);
                assert_eq!(String::from_utf8_lossy(&stdout), "granted-content");
            }
            other => panic!("expected Executed, got {other:?}"),
        }
    }

    #[test]
    fn enforce_denies_write_outside_granted_write_paths() {
        let dir = std::env::temp_dir().join(format!("pro-sandbox-test-nowrite-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("should-not-be-created.txt");

        let enforcer =
            LinuxBwrapEnforcer::with_paths(PathBuf::from("bwrap"), real_helper_path()).unwrap();
        // dir is granted read-only, not write, so the write must fail.
        let mut s = spec(&["/bin/sh", "-c", &format!("echo x > {}", target.to_str().unwrap())]);
        s.read_paths = vec![dir.clone()];

        let outcome = enforcer.enforce(&s).unwrap();
        let existed_on_host = target.exists();
        std::fs::remove_dir_all(&dir).ok();

        match outcome {
            EnforcementOutcome::Executed { exit_code, .. } => {
                assert_ne!(exit_code, 0);
                assert!(!existed_on_host, "write should not have reached the host filesystem");
            }
            other => panic!("expected Executed, got {other:?}"),
        }
    }

    #[test]
    fn enforce_allows_write_within_granted_write_path() {
        let dir = std::env::temp_dir().join(format!("pro-sandbox-test-write-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("out.txt");

        let enforcer =
            LinuxBwrapEnforcer::with_paths(PathBuf::from("bwrap"), real_helper_path()).unwrap();
        let mut s = spec(&["/bin/sh", "-c", &format!("echo written > {}", target.to_str().unwrap())]);
        s.write_paths = vec![dir.clone()];

        let outcome = enforcer.enforce(&s).unwrap();
        let host_contents = std::fs::read_to_string(&target);
        std::fs::remove_dir_all(&dir).ok();

        match outcome {
            EnforcementOutcome::Executed { exit_code, .. } => {
                assert_eq!(exit_code, 0);
                assert_eq!(host_contents.unwrap().trim(), "written");
            }
            other => panic!("expected Executed, got {other:?}"),
        }
    }

    #[test]
    fn enforce_blocks_network_entirely_when_net_hosts_empty() {
        let enforcer =
            LinuxBwrapEnforcer::with_paths(PathBuf::from("bwrap"), real_helper_path()).unwrap();
        // dash's `/dev/tcp` isn't portable, so drive a real connect via
        // python3 (already on the standard_host_binds allow-list's /usr)
        // if present; if python3 isn't installed in this environment,
        // this test can't exercise a real connect and is skipped rather
        // than asserting a false negative.
        if Command::new("python3").arg("--version").output().is_err() {
            eprintln!("skipping: python3 not available to drive a real connect() attempt");
            return;
        }
        let outcome = enforcer
            .enforce(&spec(&[
                "/usr/bin/python3",
                "-c",
                "import socket; socket.create_connection(('1.1.1.1', 80), timeout=3)",
            ]))
            .unwrap();
        match outcome {
            EnforcementOutcome::Executed { exit_code, .. } => {
                assert_ne!(exit_code, 0, "connect should fail with network namespace unshared");
            }
            other => panic!("expected Executed, got {other:?}"),
        }
    }

    #[test]
    fn landlock_status_is_fully_enforced_on_this_kernel() {
        // Correction to an earlier assumption made during planning: the
        // absence of a `/sys/kernel/security/landlock` securityfs entry
        // does NOT mean Landlock is unsupported — Landlock doesn't expose
        // a securityfs entry at all, unlike some other LSMs. Empirically
        // confirmed live in this exact WSL2 6.6 environment: requesting
        // zero Landlock rules makes even `/bin/echo` fail to exec with
        // EPERM, and granting the base host binds makes it succeed again
        // — that behavior is only possible if Landlock is genuinely
        // active. So this environment lets us live-verify the
        // `FullyEnforced` branch directly, not just the degraded one.
        //
        // This assertion is specific to a kernel that supports at least
        // Landlock ABI V3 (this one does); a kernel with no Landlock
        // support at all would instead need to see `NotEnforced` here —
        // that branch is real, written code, exercised by
        // `apply_landlock`'s own unit tests in `pro-sandbox-helper`
        // rather than asserted unconditionally in this test, since it's
        // not reproducible without an actually-Landlock-less kernel.
        let enforcer =
            LinuxBwrapEnforcer::with_paths(PathBuf::from("bwrap"), real_helper_path()).unwrap();
        let outcome = enforcer.enforce(&spec(&["/bin/echo", "hi"])).unwrap();
        match outcome {
            EnforcementOutcome::Executed { landlock, .. } => {
                assert_eq!(landlock, LandlockEnforcement::FullyEnforced { abi: 3 });
            }
            other => panic!("expected Executed, got {other:?}"),
        }
    }
}
