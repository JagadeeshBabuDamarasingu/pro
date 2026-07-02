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
    /// Reserved for a real OS-level backend: the process actually ran
    /// inside enforced isolation. No implementation in this crate
    /// constructs this variant yet.
    Executed { stdout: Vec<u8>, stderr: Vec<u8>, exit_code: i32 },
}

#[derive(Debug, thiserror::Error)]
pub enum SandboxError {
    #[error(
        "no OS-level enforcement backend is registered for this platform; refusing to run \
         '{argv0}' unsandboxed rather than silently degrading trust"
    )]
    NoBackend { argv0: String },
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
