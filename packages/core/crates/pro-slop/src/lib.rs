mod diff;
mod heuristics;
mod judge;
mod report;
mod score;

pub use diff::{parse, FileDiff, Hunk};
pub use heuristics::{duplication_ratio, scope_discipline};
pub use judge::judge_diff;
pub use report::{CodeHaluCategory, SlopAxis, SlopFinding, SlopReport};
pub use score::score;
