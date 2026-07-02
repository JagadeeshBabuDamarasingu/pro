use std::collections::HashMap;

use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::EdgeRef;

/// Standard weighted PageRank power-iteration over a directed graph whose
/// edge weights are reference counts. No external dependency — this is the
/// one piece of algorithmic logic petgraph doesn't provide itself, and it's
/// small enough not to warrant one.
pub fn pagerank<N>(graph: &DiGraph<N, u32>, damping: f64, iterations: usize) -> HashMap<NodeIndex, f64> {
    let n = graph.node_count();
    if n == 0 {
        return HashMap::new();
    }
    let base = 1.0 / n as f64;
    let mut ranks: HashMap<NodeIndex, f64> = graph.node_indices().map(|i| (i, base)).collect();

    for _ in 0..iterations {
        let mut next: HashMap<NodeIndex, f64> =
            graph.node_indices().map(|i| (i, (1.0 - damping) / n as f64)).collect();

        for node in graph.node_indices() {
            let out_edges: Vec<_> = graph.edges(node).collect();
            let out_weight: f64 = out_edges.iter().map(|e| *e.weight() as f64).sum();
            if out_weight == 0.0 {
                continue;
            }
            let share = damping * ranks[&node] / out_weight;
            for edge in out_edges {
                *next.get_mut(&edge.target()).unwrap() += share * (*edge.weight() as f64);
            }
        }
        ranks = next;
    }
    ranks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heavily_referenced_node_ranks_above_unreferenced_node() {
        let mut g = DiGraph::<&str, u32>::new();
        let hub = g.add_node("hub");
        let a = g.add_node("a");
        let b = g.add_node("b");
        let isolated = g.add_node("isolated");
        g.add_edge(a, hub, 3);
        g.add_edge(b, hub, 3);

        let ranks = pagerank(&g, 0.85, 50);
        assert!(ranks[&hub] > ranks[&isolated]);
        assert!(ranks[&hub] > ranks[&a]);
    }

    #[test]
    fn empty_graph_returns_empty_ranks() {
        let g = DiGraph::<&str, u32>::new();
        assert!(pagerank(&g, 0.85, 10).is_empty());
    }

    #[test]
    fn ranks_sum_to_approximately_one() {
        let mut g = DiGraph::<&str, u32>::new();
        let a = g.add_node("a");
        let b = g.add_node("b");
        let c = g.add_node("c");
        g.add_edge(a, b, 1);
        g.add_edge(b, c, 1);
        g.add_edge(c, a, 1);

        let ranks = pagerank(&g, 0.85, 100);
        let sum: f64 = ranks.values().sum();
        assert!((sum - 1.0).abs() < 0.01, "sum was {sum}");
    }
}
