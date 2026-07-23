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

    pub(super) fn is_branch_only_node(&self, node_id: &str) -> bool {
        self.branch_only_parents(node_id).is_some()
    }

    pub(super) fn branch_only_parents(&self, node_id: &str) -> Option<Vec<String>> {
        let mut incoming = self
            .graph
            .edges
            .iter()
            .filter(|edge| edge.to == node_id)
            .peekable();
        incoming.peek()?;

        incoming
            .map(|edge| self.is_fork_edge(edge).then(|| edge.from.clone()))
            .collect::<Option<BTreeSet<_>>>()
            .map(|parents| parents.into_iter().collect())
    }
}
