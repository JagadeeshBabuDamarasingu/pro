use serde::{Deserialize, Serialize};

/// Where a piece of data came from, tracked so the *interpreter* — not the
/// model — can refuse to let untrusted-origin data trigger privileged
/// actions. CaMeL-lite (`02-security-core.md` §2): tags every piece of data
/// with a provenance label and enforces the rule in code, not in a prompt.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Provenance {
    /// Typed or spoken directly by the user in this session.
    UserDirect,
    /// Produced by a tool this engine itself ran (e.g. compiler output,
    /// a file read from a path the user granted).
    ToolOutput { source: String },
    /// Ingested from somewhere outside the user's direct control: a web
    /// page, a PR description, an email, a third-party API response.
    Untrusted { source: String },
}

impl Provenance {
    /// Ranks provenance by how much an interpreter should trust it as the
    /// origin of a privileged action — higher rank is more restricted.
    fn trust_rank(&self) -> u8 {
        match self {
            Provenance::UserDirect => 0,
            Provenance::ToolOutput { .. } => 1,
            Provenance::Untrusted { .. } => 2,
        }
    }

    /// Combines two provenance labels, e.g. when a value is derived from
    /// two inputs of different trust — the result takes the least-trusted
    /// (highest-rank) of the two, a simple taint lattice.
    pub fn combine(a: Provenance, b: Provenance) -> Provenance {
        if a.trust_rank() >= b.trust_rank() {
            a
        } else {
            b
        }
    }

    pub fn is_untrusted(&self) -> bool {
        matches!(self, Provenance::Untrusted { .. })
    }
}

/// A value tagged with the provenance of the data that produced it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tainted<T> {
    pub value: T,
    pub provenance: Provenance,
}

impl<T> Tainted<T> {
    pub fn new(value: T, provenance: Provenance) -> Self {
        Tainted { value, provenance }
    }

    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Tainted<U> {
        Tainted { value: f(self.value), provenance: self.provenance }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ProvenanceError {
    // Field deliberately not named `source` — thiserror treats a field
    // literally named `source` as the `#[source]` error-chain field and
    // requires it to implement `std::error::Error`, which a plain `String`
    // doesn't.
    #[error(
        "refusing to trigger a privileged action from untrusted-origin data (origin: {origin}); \
         this requires explicit user confirmation, not model self-assessment"
    )]
    UntrustedTrigger { origin: String },
}

/// The CaMeL-lite enforcement point: a high-stakes action (privileged
/// capability elevation, `run` above tier 0, a production write) whose
/// *triggering* instruction is purely untrusted-derived is rejected here,
/// by this function, independent of anything the model itself claims about
/// the instruction's safety.
pub fn require_user_direct(provenance: &Provenance) -> Result<(), ProvenanceError> {
    match provenance {
        Provenance::Untrusted { source } => {
            Err(ProvenanceError::UntrustedTrigger { origin: source.clone() })
        }
        Provenance::UserDirect | Provenance::ToolOutput { .. } => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combine_picks_least_trusted() {
        let combined = Provenance::combine(
            Provenance::UserDirect,
            Provenance::Untrusted { source: "web page".into() },
        );
        assert!(combined.is_untrusted());
    }

    #[test]
    fn combine_of_two_trusted_stays_trusted() {
        let combined = Provenance::combine(
            Provenance::UserDirect,
            Provenance::ToolOutput { source: "compiler".into() },
        );
        assert!(!combined.is_untrusted());
    }

    #[test]
    fn require_user_direct_rejects_untrusted() {
        let result = require_user_direct(&Provenance::Untrusted { source: "PR description".into() });
        assert!(matches!(result, Err(ProvenanceError::UntrustedTrigger { .. })));
    }

    #[test]
    fn require_user_direct_allows_tool_output() {
        let result = require_user_direct(&Provenance::ToolOutput { source: "test runner".into() });
        assert!(result.is_ok());
    }
}
