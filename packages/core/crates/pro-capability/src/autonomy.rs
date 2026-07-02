use serde::{Deserialize, Serialize};

/// Per-capability autonomy tier. Every newly configured capability defaults
/// to `Shadow` (tier 0); there is no API path from `Shadow` straight to
/// `AutoReversible`/`AutoIrreversible` without an explicit, audit-logged
/// human sign-off. See `02-security-core.md` §1.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Default)]
#[serde(rename_all = "snake_case")]
pub enum AutonomyTier {
    #[default]
    Shadow = 0,
    Suggest = 1,
    SuggestApprove = 2,
    AutoReversible = 3,
    AutoIrreversible = 4,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Reversibility {
    Reversible,
    Irreversible,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BlastRadiusClass {
    Small,
    Medium,
    Large,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SignalQuality {
    High,
    Low,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TimeSensitivity {
    Low,
    High,
}

/// The four factors that compute a capability's default autonomy *ceiling*
/// — a cap the model's own confidence can never exceed, per
/// `02-security-core.md` §1: "an irreversible, large-blast-radius,
/// low-signal action is capped at tier 1 no matter how confident the model
/// claims to be."
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct AutonomyMetadata {
    pub reversibility: Reversibility,
    pub blast_radius: BlastRadiusClass,
    pub signal_quality: SignalQuality,
    pub time_sensitivity: TimeSensitivity,
}

impl AutonomyMetadata {
    pub fn autonomy_ceiling(&self) -> AutonomyTier {
        let mut ceiling = AutonomyTier::AutoIrreversible;

        if self.reversibility == Reversibility::Irreversible {
            ceiling = ceiling.min(AutonomyTier::AutoReversible);
        }
        if self.blast_radius == BlastRadiusClass::Large {
            ceiling = ceiling.min(AutonomyTier::SuggestApprove);
        }
        if self.signal_quality == SignalQuality::Low {
            ceiling = ceiling.min(AutonomyTier::Suggest);
        }
        // Explicit worked example from the spec: irreversible + large blast
        // radius + low signal caps at tier 1 regardless of any individual
        // factor computed above.
        if self.reversibility == Reversibility::Irreversible
            && self.blast_radius == BlastRadiusClass::Large
            && self.signal_quality == SignalQuality::Low
        {
            ceiling = ceiling.min(AutonomyTier::Suggest);
        }

        ceiling
    }
}

/// A numeric bound on the consequence of a production-write action,
/// declared at grant time and enforced at the tool-adapter layer — not
/// merely implied by overall model trust. Closes the Step Finance/Meta
/// failure pattern documented in `02-security-core.md` §1.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct BlastRadiusBound {
    pub max_hosts: Option<u32>,
    pub max_value_usd: Option<f64>,
    pub max_records: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(
        reversibility: Reversibility,
        blast_radius: BlastRadiusClass,
        signal_quality: SignalQuality,
    ) -> AutonomyMetadata {
        AutonomyMetadata {
            reversibility,
            blast_radius,
            signal_quality,
            time_sensitivity: TimeSensitivity::Low,
        }
    }

    #[test]
    fn best_case_allows_full_autonomy() {
        let m = meta(Reversibility::Reversible, BlastRadiusClass::Small, SignalQuality::High);
        assert_eq!(m.autonomy_ceiling(), AutonomyTier::AutoIrreversible);
    }

    #[test]
    fn irreversible_large_low_signal_caps_at_suggest() {
        let m = meta(Reversibility::Irreversible, BlastRadiusClass::Large, SignalQuality::Low);
        assert_eq!(m.autonomy_ceiling(), AutonomyTier::Suggest);
    }

    #[test]
    fn irreversible_alone_caps_below_auto_irreversible() {
        let m = meta(Reversibility::Irreversible, BlastRadiusClass::Small, SignalQuality::High);
        assert_eq!(m.autonomy_ceiling(), AutonomyTier::AutoReversible);
    }

    #[test]
    fn tier_ordering_is_monotonic() {
        assert!(AutonomyTier::Shadow < AutonomyTier::Suggest);
        assert!(AutonomyTier::Suggest < AutonomyTier::SuggestApprove);
        assert!(AutonomyTier::SuggestApprove < AutonomyTier::AutoReversible);
        assert!(AutonomyTier::AutoReversible < AutonomyTier::AutoIrreversible);
    }
}
