use serde::{Deserialize, Serialize};

/// Outcome of the in-sandbox helper's attempt to construct and enforce a
/// Landlock ruleset before exec'ing into the real target. Four states, not
/// a bool, because collapsing "kernel doesn't support Landlock at all" and
/// "kernel supports an older/narrower ABI than requested" into a single
/// "not fully enforced" bit would make it impossible to tell a genuinely
/// degraded host from one where every requested rule applied.
///
/// Named `LandlockEnforcement`, not `LandlockStatus`, to avoid colliding
/// with the `landlock` crate's own `LandlockStatus` (which describes
/// kernel-level Landlock *support*, not ruleset enforcement outcome —
/// `apply_landlock` folds both into this one type via `RestrictionStatus`).
///
/// Constructed only by `pro-sandbox-helper`'s `apply_landlock` — never
/// guessed at by `LinuxBwrapEnforcer` from indirect signals like bwrap's
/// exit code. Lives at the crate root, not under the `linux` module, even
/// though only the Linux backend constructs it today — it's pure data (no
/// OS-specific code, no dependency on the `landlock` crate), and it's a
/// field on the platform-agnostic `EnforcementOutcome`, so it must stay
/// available on every platform this crate compiles for.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum LandlockEnforcement {
    /// Every requested rule (all `--ro`/`--rw` paths) was installed and
    /// the kernel's Landlock ABI supported every access-right kind asked
    /// for — maps 1:1 from the `landlock` crate's own
    /// `RulesetStatus::FullyEnforced`.
    FullyEnforced { abi: u32 },
    /// Landlock is available but some requested access right or rule
    /// could not be installed at the running kernel's ABI level — some
    /// restriction the caller asked for is not actually active. Not a
    /// hard failure (bwrap's namespace isolation still holds), but must
    /// never be reported as `FullyEnforced`.
    PartiallyEnforced { abi: u32, detail: String },
    /// The kernel has no usable Landlock support at all — LSM disabled at
    /// build/boot time, or the syscalls returned ENOSYS/EOPNOTSUPP.
    /// (Note: a missing `/sys/kernel/security/landlock` securityfs entry
    /// is *not* a reliable signal for this — Landlock doesn't expose one
    /// at all, unlike some other LSMs; the only reliable check is
    /// attempting a real ruleset and reading back its actual status, as
    /// `apply_landlock` does.) bwrap's namespace/mount isolation is still
    /// the active boundary; this is a defense-in-depth downgrade, not
    /// "ran unsandboxed."
    NotEnforced { reason: String },
    /// The helper process itself never got far enough to attempt a
    /// Landlock ruleset (e.g. an unexpected error building the ruleset
    /// object itself, distinct from the kernel simply not supporting it).
    /// Kept separate from `NotEnforced` so "Landlock tried and the kernel
    /// said no" is never conflated with "Landlock was never attempted."
    HelperFailedBeforeAttempt { detail: String },
}
