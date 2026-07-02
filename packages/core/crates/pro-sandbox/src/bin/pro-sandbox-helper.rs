//! Runs *inside* the bwrap sandbox namespace set up by `LinuxBwrapEnforcer`.
//! Applies a best-effort Landlock ruleset scoped to the granted read/write
//! paths, records the honest outcome to a status file, then execs into the
//! real target — process-image replacement is what makes the installed
//! Landlock restriction (inherited across `execve`) actually govern the
//! sandboxed command. See `pro_sandbox::linux::enforcer` for the caller.
//!
//! Not cfg-gated as a `[[bin]]` target (Cargo has no such mechanism), but
//! its real logic only exists on Linux, since it depends on the `landlock`
//! crate, which is a Linux-only dependency. On any other target this
//! compiles to an inert stub so the rest of the workspace still builds.

fn main() {
    #[cfg(target_os = "linux")]
    linux_main::run();

    #[cfg(not(target_os = "linux"))]
    {
        eprintln!("pro-sandbox-helper: only supported on Linux");
        std::process::exit(1);
    }
}

#[cfg(target_os = "linux")]
mod linux_main {
    use std::os::unix::process::CommandExt;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    use landlock::{
        path_beneath_rules, Access, AccessFs, CompatLevel, Compatible, Ruleset, RulesetAttr,
        RulesetCreatedAttr, RulesetStatus, ABI,
    };
    use pro_sandbox::LandlockEnforcement;

    /// Conservative floor, broadly available since Linux 6.2 — this
    /// backend only needs plain filesystem read/write/execute rights, not
    /// the network/ioctl/truncate-specific features later ABI versions
    /// add. `CompatLevel::BestEffort` (below) means requesting a higher
    /// ABI here would still degrade gracefully on an older kernel, but
    /// there's no feature this backend needs from a newer ABI either.
    const LANDLOCK_ABI: ABI = ABI::V3;

    pub(super) fn run() -> ! {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let parsed = match parse_args(&args) {
            Ok(p) => p,
            Err(msg) => {
                eprintln!("pro-sandbox-helper: bad arguments: {msg}");
                // No status file to write to yet (parsing may have failed
                // before finding one) — the outer enforcer's
                // `read_status_file` correctly surfaces
                // `HelperStatusUnreadable` for this case.
                std::process::exit(64); // EX_USAGE
            }
        };

        let status = apply_landlock(&parsed.ro_paths, &parsed.rw_paths);
        write_status(&parsed.status_file, &status);

        // Process-image replacement — the Landlock restriction installed
        // above (inherited across execve) now governs the real target.
        let err = Command::new(&parsed.argv[0]).args(&parsed.argv[1..]).exec();
        eprintln!("pro-sandbox-helper: exec into {:?} failed: {err}", parsed.argv);
        std::process::exit(126);
    }

    struct ParsedArgs {
        ro_paths: Vec<PathBuf>,
        rw_paths: Vec<PathBuf>,
        status_file: PathBuf,
        argv: Vec<String>,
    }

    /// Parses `--ro <path>` (repeatable), `--rw <path>` (repeatable),
    /// `--status-file <path>` (required, exactly once), then a literal
    /// `--` followed by the real target argv (must be non-empty).
    fn parse_args(args: &[String]) -> Result<ParsedArgs, String> {
        let mut ro_paths = Vec::new();
        let mut rw_paths = Vec::new();
        let mut status_file: Option<PathBuf> = None;
        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "--ro" => {
                    let v = args.get(i + 1).ok_or("--ro requires a path argument")?;
                    ro_paths.push(PathBuf::from(v));
                    i += 2;
                }
                "--rw" => {
                    let v = args.get(i + 1).ok_or("--rw requires a path argument")?;
                    rw_paths.push(PathBuf::from(v));
                    i += 2;
                }
                "--status-file" => {
                    if status_file.is_some() {
                        return Err("--status-file specified more than once".to_string());
                    }
                    let v = args.get(i + 1).ok_or("--status-file requires a path argument")?;
                    status_file = Some(PathBuf::from(v));
                    i += 2;
                }
                "--" => {
                    i += 1;
                    break;
                }
                other => return Err(format!("unrecognized argument: {other}")),
            }
        }
        let status_file = status_file.ok_or("missing required --status-file")?;
        let argv: Vec<String> = args[i..].to_vec();
        if argv.is_empty() {
            return Err("missing target command after --".to_string());
        }
        Ok(ParsedArgs { ro_paths, rw_paths, status_file, argv })
    }

    /// Builds a Landlock ruleset scoped to exactly `ro_paths` (read-only
    /// access rights) and `rw_paths` (read+write), using
    /// `CompatLevel::BestEffort` (matches the `landlock` crate's own
    /// reference `sandboxer.rs` example) so a kernel with no/partial
    /// Landlock support degrades gracefully instead of erroring — the
    /// crate's own `RulesetStatus`/`LandlockStatus` are mapped onto our
    /// `LandlockEnforcement` rather than re-derived from guesswork.
    fn apply_landlock(ro_paths: &[PathBuf], rw_paths: &[PathBuf]) -> LandlockEnforcement {
        let result = Ruleset::default()
            .set_compatibility(CompatLevel::BestEffort)
            .handle_access(AccessFs::from_all(LANDLOCK_ABI))
            .and_then(|r| r.create())
            .and_then(|r| {
                r.add_rules(path_beneath_rules(ro_paths, AccessFs::from_read(LANDLOCK_ABI)))
            })
            .and_then(|r| {
                r.add_rules(path_beneath_rules(rw_paths, AccessFs::from_all(LANDLOCK_ABI)))
            })
            .and_then(|r| r.restrict_self());

        match result {
            Ok(status) => {
                let abi = LANDLOCK_ABI as u32;
                match status.ruleset {
                    RulesetStatus::FullyEnforced => LandlockEnforcement::FullyEnforced { abi },
                    RulesetStatus::PartiallyEnforced => LandlockEnforcement::PartiallyEnforced {
                        abi,
                        detail: format!("kernel Landlock support: {:?}", status.landlock),
                    },
                    RulesetStatus::NotEnforced => LandlockEnforcement::NotEnforced {
                        reason: format!("kernel Landlock support: {:?}", status.landlock),
                    },
                }
            }
            Err(e) => LandlockEnforcement::HelperFailedBeforeAttempt { detail: e.to_string() },
        }
    }

    fn write_status(path: &Path, status: &LandlockEnforcement) {
        let json = serde_json::to_vec(status).expect("LandlockEnforcement always serializes");
        // Best-effort: a write failure here is exactly what produces a
        // `HelperStatusUnreadable` on the outer read side, which is the
        // correct, honest outer-facing error.
        let _ = std::fs::write(path, json);
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn parse_args_splits_ro_rw_status_file_and_trailing_argv() {
            let args: Vec<String> = [
                "--ro", "/a", "--rw", "/b", "--status-file", "/s.json", "--", "/bin/echo", "hi",
            ]
            .into_iter()
            .map(String::from)
            .collect();
            let parsed = parse_args(&args).unwrap();
            assert_eq!(parsed.ro_paths, vec![PathBuf::from("/a")]);
            assert_eq!(parsed.rw_paths, vec![PathBuf::from("/b")]);
            assert_eq!(parsed.status_file, PathBuf::from("/s.json"));
            assert_eq!(parsed.argv, vec!["/bin/echo".to_string(), "hi".to_string()]);
        }

        #[test]
        fn parse_args_rejects_missing_status_file_flag() {
            let args: Vec<String> =
                ["--", "/bin/echo"].into_iter().map(String::from).collect();
            assert!(parse_args(&args).is_err());
        }

        #[test]
        fn parse_args_rejects_empty_trailing_argv() {
            let args: Vec<String> =
                ["--status-file", "/s.json", "--"].into_iter().map(String::from).collect();
            assert!(parse_args(&args).is_err());
        }

        #[test]
        fn parse_args_rejects_duplicate_status_file_flag() {
            let args: Vec<String> = [
                "--status-file", "/s1.json", "--status-file", "/s2.json", "--", "/bin/echo",
            ]
            .into_iter()
            .map(String::from)
            .collect();
            assert!(parse_args(&args).is_err());
        }

        #[test]
        fn apply_landlock_returns_fully_enforced_on_this_kernel() {
            // Correction to an earlier assumption made during planning:
            // `/sys/kernel/security/landlock` absence does not mean
            // Landlock is unsupported (it doesn't expose a securityfs
            // entry at all). Empirically confirmed live in this WSL2 6.6
            // environment via `LinuxBwrapEnforcer`'s own live tests: this
            // kernel genuinely enforces Landlock at ABI V3. A ruleset with
            // zero granted paths is still a *successfully installed*
            // deny-all ruleset — `RulesetStatus::FullyEnforced`, not
            // `NotEnforced` — since every requested restriction (the
            // handled access classes) was actually installed by the
            // kernel; there just happen to be no rules permitting
            // anything. On a genuinely Landlock-less kernel this would
            // instead report `NotEnforced` — real, written code, just
            // not reproducible without such a kernel to test it on.
            let status = apply_landlock(&[], &[]);
            assert_eq!(status, LandlockEnforcement::FullyEnforced { abi: 3 });
        }
    }
}
