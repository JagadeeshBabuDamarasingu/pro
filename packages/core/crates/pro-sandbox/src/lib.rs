use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// The fully-scoped shape of a requested subprocess execution: everything
/// a real OS-level enforcer (bubblewrap/Landlock, Seatbelt, AppContainer)
/// would need to build sandboxed bind mounts / namespaces / an env
/// allow-list from. Deliberately flat and serializable so it can be logged
/// verbatim to the audit trail.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunSpec {
    pub argv: Vec<String>,
    pub read_paths: Vec<PathBuf>,
    pub write_paths: Vec<PathBuf>,
    pub env_vars: Vec<String>,
    pub net_hosts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EnforcementOutcome {
    /// The enforcer recorded intent but never executed anything — the only
    /// outcome `ShadowEnforcer` can produce, and the correct behavior for
    /// tier-0 (shadow/observe-only) capabilities.
    Observed { would_have_run: RunSpec },
    /// The process actually ran inside enforced isolation.
    Executed {
        stdout: Vec<u8>,
        stderr: Vec<u8>,
        exit_code: i32,
        /// Honest, typed record of whether the additional Landlock
        /// path-restriction layer actually applied — never inferred or
        /// assumed. See `linux::landlock_status::LandlockEnforcement`.
        /// This field lives on the platform-agnostic `EnforcementOutcome`
        /// rather than under the `linux` module so it stays nameable from
        /// here; a future macOS/Windows backend may need to generalize it
        /// (e.g. to a `Vec<DefenseInDepthLayer>`) — deferred, not solved
        /// now, since no other backend exists yet.
        landlock: LandlockEnforcement,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum SandboxError {
    #[error(
        "no OS-level enforcement backend is registered for this platform; refusing to run \
         '{argv0}' unsandboxed rather than silently degrading trust"
    )]
    NoBackend { argv0: String },

    /// bwrap can only fully block (`--unshare-net`) or fully share the
    /// host netns — no per-host allow-listing without a separate netns +
    /// nftables/proxy layer, which is out of scope for this backend. A
    /// non-empty `net_hosts` must never silently degrade to full network
    /// access, so this is a hard error raised before anything is spawned.
    #[error(
        "per-host network filtering is not implemented by this backend; requested hosts \
         {hosts:?} would require either full network access (silently over-granting) or full \
         network block (silently under-granting) — refusing rather than guessing"
    )]
    PerHostNetworkFilteringUnsupported { hosts: Vec<String> },

    /// `bwrap` wasn't found on `PATH` (or a configured override path) at
    /// backend-construction time. Mirrors `pro_lsp::LspError::ServerNotFound`
    /// — map `io::ErrorKind::NotFound` from a failed spawn, never
    /// which-preflight-and-skip.
    #[error("bubblewrap ('bwrap') binary not found: {0}")]
    BackendBinaryNotFound(String),

    /// Failed to spawn `bwrap` for a reason other than "not found" (e.g.
    /// permission denied, resource exhaustion).
    #[error("failed to spawn bubblewrap: {0}")]
    Spawn(#[source] std::io::Error),

    /// The `pro-sandbox-helper` binary wasn't found next to the current
    /// executable at backend-construction time.
    #[error("pro-sandbox-helper binary not found at expected path: {0}")]
    HelperBinaryNotFound(std::path::PathBuf),

    /// bwrap exited but the helper never wrote (or partially wrote) its
    /// status file — e.g. it crashed before writing, or the exec into the
    /// real target failed before that point. Distinguished from a normal
    /// nonzero exit code from the *sandboxed target itself*, which is a
    /// valid `EnforcementOutcome::Executed { exit_code, .. }`, not an error.
    #[error("sandbox helper did not report a Landlock status: {0}")]
    HelperStatusUnreadable(String),

    /// Failed to create or prepare the status-file scratch path.
    #[error("failed to prepare sandbox status file at {path}: {source}")]
    StatusFileIo { path: std::path::PathBuf, #[source] source: std::io::Error },
}

/// The enforcement layer: every `run` capability must execute through an
/// implementation of this trait, never bypassed by the policy layer having
/// already approved it (`02-security-core.md` §1 — "a crafted command that
/// fools the policy layer is still blocked by the kernel").
pub trait SandboxEnforcer: Send + Sync {
    fn enforce(&self, spec: &RunSpec) -> Result<EnforcementOutcome, SandboxError>;
}

/// The only enforcer shipped in this pass. Never executes `spec` — always
/// records what it would have run. This is not a placeholder standing in
/// for a "real" enforcer; it *is* the correct, complete implementation for
/// tier-0 (shadow/observe-only), which every newly configured capability
/// defaults to.
#[derive(Debug, Default)]
pub struct ShadowEnforcer;

impl SandboxEnforcer for ShadowEnforcer {
    fn enforce(&self, spec: &RunSpec) -> Result<EnforcementOutcome, SandboxError> {
        Ok(EnforcementOutcome::Observed { would_have_run: spec.clone() })
    }
}

/// Tiers at or below this always route through `ShadowEnforcer` regardless
/// of whether a real backend is registered — shadow mode never needs one.
const MAX_SHADOW_ONLY_TIER: u8 = 0;

/// Looks up the enforcer that should handle a request at a given autonomy
/// tier (represented as its raw `u8` ordinal from `pro-capability`'s
/// `AutonomyTier`, to avoid a hard dependency edge from the enforcement
/// layer back onto the policy crate). Tier 0 always resolves to the shadow
/// enforcer; any higher tier requires a real backend to be registered, and
/// hard-errors if one isn't — this is the deliberate stop that keeps a
/// missing sandbox implementation from silently becoming "run unsandboxed".
pub struct EnforcerRegistry {
    shadow: ShadowEnforcer,
    real_backend: Option<Box<dyn SandboxEnforcer>>,
}

impl EnforcerRegistry {
    pub fn new() -> Self {
        EnforcerRegistry { shadow: ShadowEnforcer, real_backend: None }
    }

    pub fn with_backend(backend: Box<dyn SandboxEnforcer>) -> Self {
        EnforcerRegistry { shadow: ShadowEnforcer, real_backend: Some(backend) }
    }

    pub fn enforcer_for_tier(&self, tier_ordinal: u8) -> Result<&dyn SandboxEnforcer, SandboxError> {
        if tier_ordinal == MAX_SHADOW_ONLY_TIER {
            return Ok(&self.shadow);
        }
        self.real_backend
            .as_deref()
            .ok_or(SandboxError::NoBackend { argv0: String::new() })
    }
}

impl Default for EnforcerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

mod landlock_status;
pub use landlock_status::LandlockEnforcement;

/// Linux-only real backend: bubblewrap for namespace/mount isolation,
/// Landlock for defense-in-depth path restriction. Kept behind a `cfg` so
/// macOS/Windows backends can be sibling modules later without touching
/// this one.
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::enforcer::LinuxBwrapEnforcer;

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> RunSpec {
        RunSpec {
            argv: vec!["/bin/echo".into(), "hi".into()],
            read_paths: vec![],
            write_paths: vec![],
            env_vars: vec![],
            net_hosts: vec![],
        }
    }

    #[test]
    fn shadow_enforcer_never_executes() {
        let outcome = ShadowEnforcer.enforce(&spec()).unwrap();
        assert!(matches!(outcome, EnforcementOutcome::Observed { .. }));
    }

    #[test]
    fn registry_routes_tier_zero_to_shadow_with_no_backend() {
        let registry = EnforcerRegistry::new();
        let enforcer = registry.enforcer_for_tier(0).expect("tier 0 always resolves");
        let outcome = enforcer.enforce(&spec()).unwrap();
        assert!(matches!(outcome, EnforcementOutcome::Observed { .. }));
    }

    #[test]
    fn registry_errors_on_higher_tier_with_no_backend() {
        let registry = EnforcerRegistry::new();
        let result = registry.enforcer_for_tier(3);
        assert!(matches!(result, Err(SandboxError::NoBackend { .. })));
    }
}
