use std::collections::HashSet;

/// Token-set Jaccard similarity between two review verdicts — the
/// orthogonal mode's disagreement signal. Hand-rolled rather than a
/// cross-crate dependency on `pro-confidence`'s copy: different pipeline
/// concern (review-verdict divergence vs. sampling-consistency), same tiny
/// bit of math.
pub fn jaccard_similarity(a: &str, b: &str) -> f32 {
    let set_a: HashSet<&str> = a.split_whitespace().collect();
    let set_b: HashSet<&str> = b.split_whitespace().collect();
    if set_a.is_empty() && set_b.is_empty() {
        return 1.0;
    }
    let intersection = set_a.intersection(&set_b).count();
    let union = set_a.union(&set_b).count().max(1);
    intersection as f32 / union as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_text_has_similarity_one() {
        assert_eq!(jaccard_similarity("looks good to me", "looks good to me"), 1.0);
    }

    #[test]
    fn disjoint_text_has_similarity_zero() {
        assert_eq!(jaccard_similarity("looks good", "found a bug"), 0.0);
    }
}
