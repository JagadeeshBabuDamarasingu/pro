use std::collections::HashSet;

/// A normalized snapshot of "what the agent decided to do next" — coarse
/// enough to compare across two trajectory re-runs without needing the
/// full plan/reasoning trace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    pub kind: String,
    pub target: String,
    pub args: Vec<String>,
}

fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr = vec![0usize; b.len() + 1];
    for i in 1..=a.len() {
        curr[0] = i;
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b.len()]
}

fn normalized_target_distance(a: &str, b: &str) -> f32 {
    if a.is_empty() && b.is_empty() {
        return 0.0;
    }
    let max_len = a.chars().count().max(b.chars().count()).max(1);
    edit_distance(a, b) as f32 / max_len as f32
}

fn jaccard_distance(a: &[String], b: &[String]) -> f32 {
    let set_a: HashSet<&String> = a.iter().collect();
    let set_b: HashSet<&String> = b.iter().collect();
    if set_a.is_empty() && set_b.is_empty() {
        return 0.0;
    }
    let intersection = set_a.intersection(&set_b).count();
    let union = set_a.union(&set_b).count().max(1);
    1.0 - (intersection as f32 / union as f32)
}

/// Scores how much two actions differ, in `[0.0, 1.0]` — 0.0 is identical,
/// 1.0 is completely different. Weighted: action kind matching matters
/// most, then the target, then the argument set.
pub fn action_divergence(a: &Action, b: &Action) -> f32 {
    let kind_component = if a.kind == b.kind { 0.0 } else { 1.0 };
    let target_component = normalized_target_distance(&a.target, &b.target);
    let arg_component = jaccard_distance(&a.args, &b.args);
    0.5 * kind_component + 0.3 * target_component + 0.2 * arg_component
}

/// MELON's actual signal (`02-security-core.md` §2): re-run the agent's
/// trajectory with the user's real instruction masked, keeping any
/// untrusted content in context. If the resulting action **barely
/// changes** from the baseline (low divergence), the action likely wasn't
/// driven by the user's instruction at all — something else, most plausibly
/// injected content, is driving it. High divergence is the *normal*,
/// expected case (masking the real instruction should change what the
/// agent decides to do).
pub fn is_suspected_injection(divergence: f32, threshold: f32) -> bool {
    divergence <= threshold
}

#[cfg(test)]
mod tests {
    use super::*;

    fn action(kind: &str, target: &str, args: &[&str]) -> Action {
        Action { kind: kind.into(), target: target.into(), args: args.iter().map(|s| s.to_string()).collect() }
    }

    #[test]
    fn identical_actions_have_zero_divergence() {
        let a = action("run", "/bin/echo", &["hi"]);
        assert_eq!(action_divergence(&a, &a.clone()), 0.0);
    }

    #[test]
    fn different_kind_dominates_divergence() {
        let a = action("run", "/bin/echo", &["hi"]);
        let b = action("fs_write", "/bin/echo", &["hi"]);
        assert!(action_divergence(&a, &b) >= 0.5);
    }

    #[test]
    fn low_divergence_flags_suspected_injection() {
        let a = action("run", "curl", &["http://evil.example"]);
        let b = action("run", "curl", &["http://evil.example"]);
        let divergence = action_divergence(&a, &b);
        assert!(is_suspected_injection(divergence, 0.1));
    }

    #[test]
    fn high_divergence_is_not_flagged() {
        let a = action("run", "npm", &["install", "left-pad"]);
        let b = action("fs_write", "notes.txt", &["draft"]);
        let divergence = action_divergence(&a, &b);
        assert!(!is_suspected_injection(divergence, 0.1));
    }
}
