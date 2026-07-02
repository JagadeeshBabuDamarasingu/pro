mod melon;
mod provenance;
mod quarantine;

pub use melon::{action_divergence, is_suspected_injection, Action};
pub use provenance::{require_user_direct, Provenance, ProvenanceError, Tainted};
pub use quarantine::Quarantine;
