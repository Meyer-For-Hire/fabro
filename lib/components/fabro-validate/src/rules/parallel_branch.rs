use std::collections::BTreeSet;

use fabro_graphviz::graph::{Edge, Graph};

pub(super) struct ParallelBranches<'a> {
    graph:    &'a Graph,
    fork_ids: BTreeSet<&'a str>,
}

impl<'a> ParallelBranches<'a> {
    pub(super) fn new(graph: &'a Graph) -> Self {
        let fork_ids = graph
            .nodes
            .values()
            .filter(|node| node.handler_type() == Some("parallel"))
            .map(|node| node.id.as_str())
            .collect();
        Self { graph, fork_ids }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.fork_ids.is_empty()
    }

    pub(super) fn is_fork_edge(&self, edge: &Edge) -> bool {
        self.fork_ids.contains(edge.from.as_str())
    }

    pub(super) fn branch_targets(&self) -> BTreeSet<&str> {
        self.graph
            .edges
            .iter()
            .filter(|edge| self.is_fork_edge(edge))
            .map(|edge| edge.to.as_str())
            .collect()
    }

    /// True when every incoming edge of `node_id` comes from a parallel fork
    /// (and there is at least one). Such a node only ever runs as a branch.
    pub(super) fn is_branch_only_node(&self, node_id: &str) -> bool {
        let incoming = self.graph.incoming_edges(node_id);
        !incoming.is_empty() && incoming.iter().all(|edge| self.is_fork_edge(edge))
    }

    /// The sorted, deduplicated fork parents of a branch-only node, or `None`
    /// when the node has a non-fork entry path (or no entry at all).
    pub(super) fn branch_only_parents(&self, node_id: &str) -> Option<Vec<String>> {
        if !self.is_branch_only_node(node_id) {
            return None;
        }
        let parents: BTreeSet<&str> = self
            .graph
            .incoming_edges(node_id)
            .into_iter()
            .map(|edge| edge.from.as_str())
            .collect();
        Some(parents.into_iter().map(String::from).collect())
    }
}
