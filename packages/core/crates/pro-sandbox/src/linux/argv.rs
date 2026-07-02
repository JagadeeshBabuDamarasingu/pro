use std::path::{Path, PathBuf};

use crate::RunSpec;

/// Fixed in-sandbox path the helper binary is bound to, regardless of
/// where it lives on the host — it isn't one of the caller's granted
/// read/write paths, so it gets its own reserved location rather than an
/// identity bind.
pub(crate) const HELPER_SANDBOX_PATH: &str = "/pro-sandbox-helper";
/// Fixed in-sandbox path the status-file scratch file is bound to, for the
/// same reason as `HELPER_SANDBOX_PATH`.
pub(crate) const STATUS_FILE_SANDBOX_PATH: &str = "/pro-sandbox-status";

/// The standard minimal host environment bwrap needs to exec anything at
/// all (a real interpreter/loader, shared libs, DNS, TLS trust store).
/// Filters to paths that actually exist on *this* host — e.g. `/lib64`
/// doesn't exist on every distro — so `build_bwrap_argv` never emits a
/// `--ro-bind` for a source that isn't there (which is a bwrap hard
/// error, not a soft skip). This is the one function in this module that
/// touches the filesystem; kept separate from the pure argv builder below
/// so the builder itself stays trivially unit-testable with a fixed list.
pub(crate) fn standard_host_binds() -> Vec<PathBuf> {
    ["/usr", "/bin", "/lib", "/lib64", "/etc/resolv.conf", "/etc/ssl"]
        .into_iter()
        .map(PathBuf::from)
        .filter(|p| p.exists())
        .collect()
}

/// Pure, deterministic bwrap argv construction — no filesystem or process
/// access, so this is exhaustively unit-tested without ever invoking real
/// `bwrap`. `host_binds` (see `standard_host_binds`) and `resolved_env`
/// (name/value pairs already looked up against the real process env) are
/// passed in rather than probed/read here, keeping every side effect out
/// of the argument-shape logic under test.
///
/// All of `spec.read_paths`/`spec.write_paths` are bind-mounted at their
/// own host path unchanged (identity mapping) — `spec.argv[0]` is already
/// validated by `pro-core` against a granted `RunBinary.path`, so it needs
/// no rewriting to remain valid inside the sandbox. Only the helper binary
/// and the status file get a dedicated in-sandbox path, since neither is
/// one of the caller's granted paths.
pub(crate) fn build_bwrap_argv(
    spec: &RunSpec,
    host_binds: &[PathBuf],
    helper_host_path: &Path,
    status_file_host_path: &Path,
    resolved_env: &[(String, String)],
) -> Vec<String> {
    let mut argv: Vec<String> = Vec::new();

    // Isolate every namespace kind bwrap knows how to unshare in one shot
    // (user, ipc, pid, net, uts, cgroup) — this is what makes network
    // fully blocked by default, and matches the exact invocation already
    // live-verified to work in this dev environment.
    push_str(&mut argv, "--unshare-all");
    push_str(&mut argv, "--die-with-parent");

    push_str(&mut argv, "--proc");
    push_str(&mut argv, "/proc");
    push_str(&mut argv, "--dev");
    push_str(&mut argv, "/dev");
    // Mounted before any bind below so a write path under host `/tmp`
    // layers its bind mount on top of this tmpfs rather than the tmpfs
    // wiping out an earlier bind.
    push_str(&mut argv, "--tmpfs");
    push_str(&mut argv, "/tmp");

    for p in host_binds {
        push_str(&mut argv, "--ro-bind");
        push_path(&mut argv, p);
        push_path(&mut argv, p);
    }
    for p in &spec.read_paths {
        push_str(&mut argv, "--ro-bind");
        push_path(&mut argv, p);
        push_path(&mut argv, p);
    }
    for p in &spec.write_paths {
        push_str(&mut argv, "--bind");
        push_path(&mut argv, p);
        push_path(&mut argv, p);
    }

    push_str(&mut argv, "--bind");
    push_path(&mut argv, status_file_host_path);
    push_str(&mut argv, STATUS_FILE_SANDBOX_PATH);

    push_str(&mut argv, "--ro-bind");
    push_path(&mut argv, helper_host_path);
    push_str(&mut argv, HELPER_SANDBOX_PATH);

    push_str(&mut argv, "--clearenv");
    for (name, value) in resolved_env {
        push_str(&mut argv, "--setenv");
        push_str(&mut argv, name);
        push_str(&mut argv, value);
    }

    push_str(&mut argv, "--");
    push_str(&mut argv, HELPER_SANDBOX_PATH);
    // The helper builds its own Landlock ruleset from exactly these `--ro`/
    // `--rw` flags, and that ruleset — once `restrict_self()` runs — also
    // governs the helper process itself for the remainder of its
    // lifetime (until it execs away). So this must include not just the
    // caller's granted paths but everything the helper needs after that
    // point: `host_binds` (or nothing could even exec — the target
    // binary and its dynamic linker/libs live under `/usr`/`/bin`/etc.)
    // and the status file's own in-sandbox path (the helper writes to it
    // *after* restrict_self, so it must already be a granted rw path).
    for p in host_binds {
        push_str(&mut argv, "--ro");
        push_path(&mut argv, p);
    }
    for p in &spec.read_paths {
        push_str(&mut argv, "--ro");
        push_path(&mut argv, p);
    }
    for p in &spec.write_paths {
        push_str(&mut argv, "--rw");
        push_path(&mut argv, p);
    }
    push_str(&mut argv, "--rw");
    push_str(&mut argv, STATUS_FILE_SANDBOX_PATH);
    push_str(&mut argv, "--status-file");
    push_str(&mut argv, STATUS_FILE_SANDBOX_PATH);
    push_str(&mut argv, "--");
    for a in &spec.argv {
        push_str(&mut argv, a);
    }

    argv
}

fn push_str(argv: &mut Vec<String>, s: &str) {
    argv.push(s.to_string());
}

fn push_path(argv: &mut Vec<String>, p: &Path) {
    argv.push(p.display().to_string());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(argv: &[&str], read: &[&str], write: &[&str]) -> RunSpec {
        RunSpec {
            argv: argv.iter().map(|s| s.to_string()).collect(),
            read_paths: read.iter().map(PathBuf::from).collect(),
            write_paths: write.iter().map(PathBuf::from).collect(),
            env_vars: vec![],
            net_hosts: vec![],
        }
    }

    fn build(s: &RunSpec) -> Vec<String> {
        build_bwrap_argv(
            s,
            &[PathBuf::from("/usr")],
            Path::new("/host/helper"),
            Path::new("/host/status.json"),
            &[],
        )
    }

    fn flag_value_pairs<'a>(argv: &'a [String], flag: &str) -> Vec<&'a str> {
        argv.iter()
            .zip(argv.iter().skip(1))
            .filter(|(f, _)| f.as_str() == flag)
            .map(|(_, v)| v.as_str())
            .collect()
    }

    #[test]
    fn build_bwrap_argv_includes_unshare_all_always() {
        let s = spec(&["/bin/echo"], &[], &[]);
        let argv = build(&s);
        assert!(argv.contains(&"--unshare-all".to_string()));
    }

    #[test]
    fn build_bwrap_argv_binds_read_paths_read_only() {
        let s = spec(&["/bin/echo"], &["/data/in"], &[]);
        let argv = build(&s);
        assert!(flag_value_pairs(&argv, "--ro-bind").contains(&"/data/in"));
        assert!(!flag_value_pairs(&argv, "--bind").contains(&"/data/in"));
    }

    #[test]
    fn build_bwrap_argv_never_binds_write_path_as_readonly() {
        let s = spec(&["/bin/echo"], &[], &["/data/out"]);
        let argv = build(&s);
        let ro_targets = flag_value_pairs(&argv, "--ro-bind");
        assert!(!ro_targets.contains(&"/data/out"));
        let rw_targets = flag_value_pairs(&argv, "--bind");
        assert!(rw_targets.contains(&"/data/out"));
    }

    #[test]
    fn build_bwrap_argv_never_binds_read_path_as_writable() {
        let s = spec(&["/bin/echo"], &["/data/in"], &[]);
        let argv = build(&s);
        let rw_targets = flag_value_pairs(&argv, "--bind");
        assert!(!rw_targets.contains(&"/data/in"));
    }

    #[test]
    fn build_bwrap_argv_includes_status_file_readwrite_bind() {
        let s = spec(&["/bin/echo"], &[], &[]);
        let argv = build(&s);
        // status file host path must be bound writable (--bind), not read-only
        let idx = argv.iter().position(|a| a == "--bind").expect("a --bind present");
        assert_eq!(argv[idx + 1], "/host/status.json");
        assert_eq!(argv[idx + 2], "/pro-sandbox-status");
    }

    #[test]
    fn build_bwrap_argv_includes_helper_as_readonly_entrypoint() {
        let s = spec(&["/bin/echo"], &[], &[]);
        let argv = build(&s);
        let ro_binds = flag_value_pairs(&argv, "--ro-bind");
        assert!(ro_binds.contains(&"/host/helper"));
        // the exec target passed to bwrap (after "--") is the helper, not spec.argv[0]
        let dash_dash = argv.iter().position(|a| a == "--").expect("a -- present");
        assert_eq!(argv[dash_dash + 1], HELPER_SANDBOX_PATH);
    }

    #[test]
    fn build_bwrap_argv_forwards_real_argv_after_helper_args() {
        let s = spec(&["/bin/echo", "hello", "world"], &[], &[]);
        let argv = build(&s);
        // last three entries are exactly the real command
        let tail = &argv[argv.len() - 3..];
        assert_eq!(tail, &["/bin/echo".to_string(), "hello".to_string(), "world".to_string()]);
    }

    #[test]
    fn build_bwrap_argv_forwards_helper_ro_rw_status_flags() {
        let s = spec(&["/bin/echo"], &["/data/in"], &["/data/out"]);
        let argv = build(&s);
        assert!(argv.windows(2).any(|w| w[0] == "--ro" && w[1] == "/data/in"));
        assert!(argv.windows(2).any(|w| w[0] == "--rw" && w[1] == "/data/out"));
        assert!(argv.windows(2).any(|w| w[0] == "--status-file" && w[1] == "/pro-sandbox-status"));
    }

    #[test]
    fn build_bwrap_argv_includes_clearenv_even_with_no_resolved_env() {
        let s = spec(&["/bin/echo"], &[], &[]);
        let argv = build_bwrap_argv(
            &s,
            &[],
            Path::new("/host/helper"),
            Path::new("/host/status.json"),
            &[],
        );
        assert!(argv.contains(&"--clearenv".to_string()));
        assert!(!argv.contains(&"--setenv".to_string()));
    }

    #[test]
    fn build_bwrap_argv_includes_setenv_for_each_resolved_env_pair() {
        let s = spec(&["/bin/echo"], &[], &[]);
        let resolved = vec![("PATH".to_string(), "/usr/bin".to_string())];
        let argv = build_bwrap_argv(
            &s,
            &[],
            Path::new("/host/helper"),
            Path::new("/host/status.json"),
            &resolved,
        );
        assert!(argv.windows(3).any(|w| w[0] == "--setenv" && w[1] == "PATH" && w[2] == "/usr/bin"));
    }

    #[test]
    fn standard_host_binds_filters_to_existing_paths_only() {
        let binds = standard_host_binds();
        for p in &binds {
            assert!(p.exists(), "{p:?} should exist since standard_host_binds filters non-existent paths");
        }
    }
}
