use fabro_graphviz::graph::Graph;

use super::parallel_branch::ParallelBranches;
use crate::{Diagnostic, LintRule, Severity};

pub(super) fn rule() -> Box<dyn LintRule> {
    Box::new(Rule)
}

struct Rule;

impl Rule {
    const FIX: &str = "Add fidelity=\"full\" to enable session reuse, or remove thread_id";
}

impl LintRule for Rule {
    fn name(&self) -> &'static str {
        "thread_id_requires_fidelity_full"
    }

    fn apply(&self, graph: &Graph) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let graph_default_full = graph.default_fidelity() == Some("full");
        let branches = ParallelBranches::new(graph);

        // thread_id is inert on parallel branches, where
        // parallel_branch_inert_attribute already says "remove thread_id" —
        // advising fidelity="full" there would contradict it.
        for node in graph.nodes.values() {
            if node.thread_id().is_some()
                && !branches.is_branch_only_node(&node.id)
                && node.fidelity() != Some("full")
                && !graph_default_full
            {
                diagnostics.push(Diagnostic {
                    rule: self.name().to_string(),
                    severity: Severity::Warning,
                    message: format!(
                        "Node '{}' has thread_id but fidelity is not 'full'",
                        node.id
                    ),
                    node_id: Some(node.id.clone()),
                    edge: None,
                    fix: Some(Self::FIX.to_string()),

                    ..Diagnostic::default()
                });
            }
        }

        for edge in &graph.edges {
            if edge.thread_id().is_some() && !branches.is_fork_edge(edge) {
                let edge_full = edge.fidelity() == Some("full");
                let target_full =
                    graph.nodes.get(&edge.to).and_then(|n| n.fidelity()) == Some("full");
                if !edge_full && !target_full && !graph_default_full {
                    diagnostics.push(Diagnostic {
                        rule: self.name().to_string(),
                        severity: Severity::Warning,
                        message: format!(
                            "Edge {} -> {} has thread_id but fidelity is not 'full'",
                            edge.from, edge.to
                        ),
                        node_id: None,
                        edge: Some((edge.from.clone(), edge.to.clone())),
                        fix: Some(Self::FIX.to_string()),

                        ..Diagnostic::default()
                    });
                }
            }
        }

        if graph.default_thread().is_some() && !graph_default_full {
            diagnostics.push(Diagnostic {
                rule: self.name().to_string(),
                severity: Severity::Warning,
                message: "Graph has default_thread but default_fidelity is not 'full'".to_string(),
                node_id: None,
                edge: None,
                fix: Some(Self::FIX.to_string()),

                ..Diagnostic::default()
            });
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use fabro_graphviz::graph::{AttrValue, Edge, Graph, Node};

    use super::Rule;
    use crate::rules::test_support::minimal_graph;
    use crate::{LintRule, Severity};

    fn parallel_graph() -> Graph {
        let mut g = minimal_graph();
        let mut fork = Node::new("fork");
        fork.attrs.insert(
            "shape".to_string(),
            AttrValue::String("component".to_string()),
        );
        g.nodes.insert("fork".to_string(), fork);
        g.nodes.insert("branch".to_string(), Node::new("branch"));
        g.edges = vec![
            Edge::new("start", "fork"),
            Edge::new("fork", "branch"),
            Edge::new("branch", "exit"),
        ];
        g
    }

    #[test]
    fn thread_id_requires_fidelity_full_node_warns() {
        let mut g = minimal_graph();
        let mut node = Node::new("work");
        node.attrs.insert(
            "thread_id".to_string(),
            AttrValue::String("session1".to_string()),
        );
        g.nodes.insert("work".to_string(), node);
        g.edges.push(Edge::new("start", "work"));
        g.edges.push(Edge::new("work", "exit"));

        let rule = Rule;
        let d = rule.apply(&g);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].severity, Severity::Warning);
        assert_eq!(d[0].node_id, Some("work".to_string()));
    }

    #[test]
    fn thread_id_requires_fidelity_full_node_ok() {
        let mut g = minimal_graph();
        let mut node = Node::new("work");
        node.attrs.insert(
            "thread_id".to_string(),
            AttrValue::String("session1".to_string()),
        );
        node.attrs.insert(
            "fidelity".to_string(),
            AttrValue::String("full".to_string()),
        );
        g.nodes.insert("work".to_string(), node);
        g.edges.push(Edge::new("start", "work"));
        g.edges.push(Edge::new("work", "exit"));

        let rule = Rule;
        let d = rule.apply(&g);
        assert!(d.is_empty());
    }

    #[test]
    fn thread_id_requires_fidelity_full_node_graph_default_ok() {
        let mut g = minimal_graph();
        g.attrs.insert(
            "default_fidelity".to_string(),
            AttrValue::String("full".to_string()),
        );
        let mut node = Node::new("work");
        node.attrs.insert(
            "thread_id".to_string(),
            AttrValue::String("session1".to_string()),
        );
        g.nodes.insert("work".to_string(), node);
        g.edges.push(Edge::new("start", "work"));
        g.edges.push(Edge::new("work", "exit"));

        let rule = Rule;
        let d = rule.apply(&g);
        assert!(d.is_empty());
    }

    #[test]
    fn thread_id_requires_fidelity_full_edge_warns() {
        let mut g = minimal_graph();
        let mut edge = Edge::new("start", "exit");
        edge.attrs.insert(
            "thread_id".to_string(),
            AttrValue::String("session1".to_string()),
        );
        g.edges = vec![edge];

        let rule = Rule;
        let d = rule.apply(&g);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].severity, Severity::Warning);
        assert_eq!(d[0].edge, Some(("start".to_string(), "exit".to_string())));
    }

    #[test]
    fn thread_id_requires_fidelity_full_edge_ok() {
        let mut g = minimal_graph();
        let mut edge = Edge::new("start", "exit");
        edge.attrs.insert(
            "thread_id".to_string(),
            AttrValue::String("session1".to_string()),
        );
        edge.attrs.insert(
            "fidelity".to_string(),
            AttrValue::String("full".to_string()),
        );
        g.edges = vec![edge];

        let rule = Rule;
        let d = rule.apply(&g);
        assert!(d.is_empty());
    }

    #[test]
    fn thread_id_requires_fidelity_full_edge_target_node_ok() {
        let mut g = minimal_graph();
        if let Some(exit_node) = g.nodes.get_mut("exit") {
            exit_node.attrs.insert(
                "fidelity".to_string(),
                AttrValue::String("full".to_string()),
            );
        }
        let mut edge = Edge::new("start", "exit");
        edge.attrs.insert(
            "thread_id".to_string(),
            AttrValue::String("session1".to_string()),
        );
        g.edges = vec![edge];

        let rule = Rule;
        let d = rule.apply(&g);
        assert!(d.is_empty());
    }

    #[test]
    fn skips_thread_id_on_parallel_branch_edge() {
        let mut g = parallel_graph();
        g.edges[1].attrs.insert(
            "thread_id".to_string(),
            AttrValue::String("branch-thread".to_string()),
        );

        assert!(Rule.apply(&g).is_empty());
    }

    #[test]
    fn skips_thread_id_on_branch_only_node() {
        let mut g = parallel_graph();
        g.nodes
            .get_mut("branch")
            .expect("graph has branch")
            .attrs
            .insert(
                "thread_id".to_string(),
                AttrValue::String("branch-thread".to_string()),
            );

        assert!(Rule.apply(&g).is_empty());
    }

    #[test]
    fn checks_thread_id_on_branch_node_with_normal_entry() {
        let mut g = parallel_graph();
        g.edges.push(Edge::new("start", "branch"));
        g.nodes
            .get_mut("branch")
            .expect("graph has branch")
            .attrs
            .insert(
                "thread_id".to_string(),
                AttrValue::String("shared-thread".to_string()),
            );

        let d = Rule.apply(&g);

        assert_eq!(d.len(), 1);
        assert_eq!(d[0].node_id.as_deref(), Some("branch"));
    }

    #[test]
    fn thread_id_requires_fidelity_full_graph_warns() {
        let mut g = minimal_graph();
        g.attrs.insert(
            "default_thread".to_string(),
            AttrValue::String("session1".to_string()),
        );

        let rule = Rule;
        let d = rule.apply(&g);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].severity, Severity::Warning);
        assert!(d[0].node_id.is_none());
        assert!(d[0].edge.is_none());
    }

    #[test]
    fn thread_id_requires_fidelity_full_graph_ok() {
        let mut g = minimal_graph();
        g.attrs.insert(
            "default_thread".to_string(),
            AttrValue::String("session1".to_string()),
        );
        g.attrs.insert(
            "default_fidelity".to_string(),
            AttrValue::String("full".to_string()),
        );

        let rule = Rule;
        let d = rule.apply(&g);
        assert!(d.is_empty());
    }
}
