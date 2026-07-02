mod gate;
mod install_script;
mod registry;
mod typosquat;

pub use gate::{Reason, SupplyChainGate, SupplyChainReport, Verdict};
pub use install_script::{scan_install_script, SuspiciousPattern};
pub use registry::{CratesIoRegistryClient, NpmRegistryClient, PublishRecord, RegistryClient, RegistryError};
pub use typosquat::{closest_typosquat_target, edit_distance, POPULAR_CRATES, POPULAR_NPM};
