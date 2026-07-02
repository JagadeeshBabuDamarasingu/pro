use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::autonomy::AutonomyTier;
use crate::autonomy::BlastRadiusBound;
use crate::capability::Capability;
use crate::hash::ContentHash;

/// An active capability grant: a scoped capability, bound to the content
/// hash of the definition that produced it, at a specific autonomy tier.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Grant {
    pub capability: Capability,
    pub content_hash: ContentHash,
    pub autonomy_tier: AutonomyTier,
    pub blast_radius: Option<BlastRadiusBound>,
    #[serde(with = "time::serde::rfc3339")]
    pub granted_at: OffsetDateTime,
}

impl Grant {
    /// Every capability instantiated from a profile or freshly parsed
    /// config starts at tier 0 (shadow/observe-only) — promotion requires a
    /// separate, explicit, audit-logged action.
    pub fn new_shadow(capability: Capability) -> Self {
        let content_hash = ContentHash::of_bytes(
            &serde_json::to_vec(&capability).expect("capability serializes"),
        );
        Grant {
            capability,
            content_hash,
            autonomy_tier: AutonomyTier::Shadow,
            blast_radius: None,
            granted_at: OffsetDateTime::now_utc(),
        }
    }
}
