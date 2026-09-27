//! Topological sort (spec §4.2, §14) and cycle recovery (spec §4.3).
//!
//! Kahn's algorithm, with a min-heap keyed on declaration index so that
//! whenever more than one node is ready at once, the earliest-declared
//! one runs first — the spec §14 tie-break that makes evaluation order
//! (and so output) deterministic regardless of how the nodes happen to
//! be stored. When nodes are left over at the end, they form at least one
//! cycle; a DFS with an explicit parent stack recovers one actual cycle
//! to report, rather than just declaring failure.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use indexmap::{IndexMap, IndexSet};

use crate::graph::{Graph, NodeId};

/// The result of one topological sort attempt: `sorted` is safe to
/// evaluate in order regardless of whether every node made it in —
/// nothing in it depends on anything left in `remaining` (spec §4.6:
/// failure containment covers a cycle exactly like any other failure,
/// so nodes independent of it still evaluate). `remaining` is empty on a
/// clean sort, and always contains at least one real cycle otherwise
/// (spec §4.3).
pub struct TopoOutcome {
    pub sorted: Vec<NodeId>,
    pub remaining: IndexSet<NodeId>,
}

pub fn topo_sort(graph: &Graph) -> TopoOutcome {
    let mut successors: IndexMap<&NodeId, Vec<&NodeId>> = IndexMap::new();
    let mut in_degree: IndexMap<&NodeId, usize> = IndexMap::new();

    for node in &graph.nodes {
        let deps = &graph.deps[node];
        in_degree.insert(node, deps.len());
        for dep in deps {
            successors.entry(dep).or_default().push(node);
        }
    }

    let mut heap: BinaryHeap<Reverse<(usize, &NodeId)>> = BinaryHeap::new();
    for node in &graph.nodes {
        if in_degree[node] == 0 {
            heap.push(Reverse((graph.decl_index[node], node)));
        }
    }

    let mut sorted = Vec::new();
    while let Some(Reverse((_, node))) = heap.pop() {
        sorted.push(node.clone());
        if let Some(succs) = successors.get(node) {
            for &succ in succs {
                let degree = in_degree
                    .get_mut(succ)
                    .expect("successor is always a graph node");
                *degree -= 1;
                if *degree == 0 {
                    heap.push(Reverse((graph.decl_index[succ], succ)));
                }
            }
        }
    }

    let remaining = graph
        .nodes
        .iter()
        .filter(|n| in_degree[*n] > 0)
        .cloned()
        .collect();
    TopoOutcome { sorted, remaining }
}

/// One real cycle among `remaining` (spec §4.3: "report a cycle as the
/// full path, with file and line for each hop"), as a sequence of nodes
/// where consecutive entries — including the last back to the first —
/// each depend on the next.
pub fn find_cycle(graph: &Graph, remaining: &IndexSet<NodeId>) -> Vec<NodeId> {
    let mut starts: Vec<&NodeId> = remaining.iter().collect();
    starts.sort_by_key(|n| graph.decl_index[*n]);

    let mut visited: IndexSet<NodeId> = IndexSet::new();
    for start in starts {
        if visited.contains(start) {
            continue;
        }
        let mut stack = Vec::new();
        let mut on_stack = IndexSet::new();
        if let Some(cycle) = dfs(
            graph,
            remaining,
            start,
            &mut visited,
            &mut stack,
            &mut on_stack,
        ) {
            return cycle;
        }
    }
    unreachable!("find_cycle is only called when Kahn's algorithm left a cycle among `remaining`")
}

fn dfs(
    graph: &Graph,
    remaining: &IndexSet<NodeId>,
    node: &NodeId,
    visited: &mut IndexSet<NodeId>,
    stack: &mut Vec<NodeId>,
    on_stack: &mut IndexSet<NodeId>,
) -> Option<Vec<NodeId>> {
    visited.insert(node.clone());
    stack.push(node.clone());
    on_stack.insert(node.clone());

    for dep in &graph.deps[node] {
        if !remaining.contains(dep) {
            continue;
        }
        if on_stack.contains(dep) {
            let start = stack
                .iter()
                .position(|n| n == dep)
                .expect("dep is on_stack, so it's on stack");
            return Some(stack[start..].to_vec());
        }
        if !visited.contains(dep)
            && let Some(cycle) = dfs(graph, remaining, dep, visited, stack, on_stack)
        {
            return Some(cycle);
        }
    }

    stack.pop();
    on_stack.shift_remove(node);
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiny_graph(edges: &[(&str, &[&str])]) -> Graph {
        let mut graph = Graph::new();
        for (i, (name, deps)) in edges.iter().enumerate() {
            let deps = deps
                .iter()
                .map(|d| NodeId::TopLevel(d.to_string()))
                .collect();
            graph.insert(NodeId::TopLevel(name.to_string()), i, deps);
        }
        graph
    }

    #[test]
    fn sorts_a_simple_chain() {
        let graph = tiny_graph(&[("a", &[]), ("b", &["a"]), ("c", &["b"])]);
        let outcome = topo_sort(&graph);
        assert!(outcome.remaining.is_empty());
        let positions: Vec<&str> = outcome
            .sorted
            .iter()
            .map(|n| match n {
                NodeId::TopLevel(name) => name.as_str(),
                _ => unreachable!(),
            })
            .collect();
        assert_eq!(positions, vec!["a", "b", "c"]);
    }

    #[test]
    fn ties_break_by_declaration_index() {
        // b and c both become ready at once (after a); declaration order
        // (b before c) must decide, regardless of hash/insertion quirks.
        let graph = tiny_graph(&[("a", &[]), ("c", &["a"]), ("b", &["a"])]);
        let outcome = topo_sort(&graph);
        assert!(outcome.remaining.is_empty());
        let positions: Vec<&str> = outcome
            .sorted
            .iter()
            .map(|n| match n {
                NodeId::TopLevel(name) => name.as_str(),
                _ => unreachable!(),
            })
            .collect();
        // Declared order was a(0), c(1), b(2); c depends on a, so a is
        // first; b and c are then both ready, and c was declared first.
        assert_eq!(positions, vec!["a", "c", "b"]);
    }

    #[test]
    fn detects_a_self_cycle() {
        let graph = tiny_graph(&[("a", &["a"])]);
        let outcome = topo_sort(&graph);
        assert!(!outcome.remaining.is_empty(), "expected a cycle");
        let cycle = find_cycle(&graph, &outcome.remaining);
        assert_eq!(cycle, vec![NodeId::TopLevel("a".to_string())]);
    }

    #[test]
    fn detects_a_two_node_cycle() {
        let graph = tiny_graph(&[("a", &["b"]), ("b", &["a"])]);
        let outcome = topo_sort(&graph);
        assert!(!outcome.remaining.is_empty(), "expected a cycle");
        let cycle = find_cycle(&graph, &outcome.remaining);
        let names: IndexSet<&str> = cycle
            .iter()
            .map(|n| match n {
                NodeId::TopLevel(name) => name.as_str(),
                _ => unreachable!(),
            })
            .collect();
        assert_eq!(cycle.len(), 2);
        assert!(names.contains("a") && names.contains("b"));
    }

    #[test]
    fn detects_a_long_cycle_ignoring_unrelated_nodes() {
        let graph = tiny_graph(&[
            ("unrelated", &[]),
            ("a", &["b"]),
            ("b", &["c"]),
            ("c", &["a"]),
        ]);
        let outcome = topo_sort(&graph);
        assert_eq!(outcome.remaining.len(), 3);
        let cycle = find_cycle(&graph, &outcome.remaining);
        assert_eq!(cycle.len(), 3);
    }
}
