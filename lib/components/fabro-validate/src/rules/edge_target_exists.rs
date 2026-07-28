use std::collections::HashSet;

use fabro_graphviz::graph::Graph;

use crate::{Diagnostic, LintRule, Severity};

pub(super) fn rule() -> Box<dyn LintRule> {
    Box::new(Rule)
}

struct Rule;

impl Rule {
    fn diagnostic(&self, node_id: &str, from: &str, to: &str) -> Diagnostic {
        Diagnostic {
            rule: self.name().to_string(),
            severity: Severity::Error,
            message: format!(
                "Node '{node_id}' is referenced by edge '{from} -> {to}' but has no node \
                 declaration"
            ),
            node_id: Some(node_id.to_string()),
            edge: Some((from.to_string(), to.to_string())),
            fix: Some(format!(
                "Declare node '{node_id}' or correct the edge endpoint"
            )),

            ..Diagnostic::default()
        }
    }
}

impl LintRule for Rule {
    fn name(&self) -> &'static str {
        "edge_target_exists"
    }

    fn apply(&self, graph: &Graph) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let mut reported = HashSet::new();
        for edge in &graph.edges {
            for endpoint in [&edge.to, &edge.from] {
                if !graph.nodes.contains_key(endpoint) && reported.insert(endpoint) {
                    diagnostics.push(self.diagnostic(endpoint, &edge.from, &edge.to));
                }
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use fabro_graphviz::graph::{Edge, Graph};
    use fabro_graphviz::parser;

    use super::Rule;
    use crate::rules::test_support::minimal_graph;
    use crate::{Diagnostic, LintRule, Severity};

    fn parse(dot: &str) -> Graph {
        parser::parse(dot).expect("fixture should parse")
    }

    fn undeclared_nodes(graph: &Graph) -> Vec<String> {
        Rule.apply(graph)
            .into_iter()
            .map(|d| d.node_id.expect("diagnostic should name a node"))
            .collect()
    }

    #[test]
    fn edge_only_node_is_rejected() {
        let graph = parse(
            r"digraph EdgeOnly {
                start [shape=Mdiamond]
                exit  [shape=Msquare]
                start -> misspelled_node
                misspelled_node -> exit
            }",
        );

        let diagnostics = Rule.apply(&graph);
        assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
        let Diagnostic {
            severity,
            node_id,
            edge,
            ..
        } = &diagnostics[0];
        assert_eq!(*severity, Severity::Error);
        assert_eq!(node_id.as_deref(), Some("misspelled_node"));
        assert_eq!(
            edge.clone(),
            Some(("start".to_string(), "misspelled_node".to_string()))
        );
    }

    #[test]
    fn declaration_after_the_edge_is_accepted() {
        let graph = parse(
            r#"digraph DeclaredLater {
                start -> work
                work [prompt="Do the work"]
                work -> exit
                start [shape=Mdiamond]
                exit  [shape=Msquare]
            }"#,
        );

        assert!(Rule.apply(&graph).is_empty());
    }

    #[test]
    fn chained_edges_report_every_undeclared_endpoint() {
        let graph = parse(
            r"digraph Chained {
                start [shape=Mdiamond]
                exit  [shape=Msquare]
                start -> first -> second -> exit
            }",
        );

        assert_eq!(undeclared_nodes(&graph), vec!["first", "second"]);
    }

    #[test]
    fn a_node_is_reported_once_no_matter_how_many_edges_use_it() {
        let graph = parse(
            r"digraph Repeated {
                start [shape=Mdiamond]
                exit  [shape=Msquare]
                start -> typo
                typo -> exit
                typo -> start
            }",
        );

        assert_eq!(undeclared_nodes(&graph), vec!["typo"]);
    }

    #[test]
    fn subgraph_declaration_is_accepted() {
        let graph = parse(
            r#"digraph Subgraphed {
                start [shape=Mdiamond]
                exit  [shape=Msquare]

                subgraph cluster_loop {
                    label = "Loop A"
                    plan [prompt="Plan the work"]
                }

                start -> plan -> exit
            }"#,
        );

        assert!(Rule.apply(&graph).is_empty());
    }

    #[test]
    fn edge_target_exists_rule_missing_target() {
        let mut g = minimal_graph();
        g.edges.push(Edge::new("start", "nonexistent"));
        let rule = Rule;
        let d = rule.apply(&g);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].severity, Severity::Error);
    }

    #[test]
    fn edge_target_exists_rule_valid() {
        let g = minimal_graph();
        let rule = Rule;
        let d = rule.apply(&g);
        assert!(d.is_empty());
    }

    #[test]
    fn edge_target_exists_rule_missing_source() {
        let mut g = minimal_graph();
        g.edges.push(Edge::new("nonexistent_source", "exit"));
        let rule = Rule;
        let d = rule.apply(&g);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].severity, Severity::Error);
        assert!(d[0].message.contains("nonexistent_source"));
    }

    // --- Additional coverage: reachability no start node ---

    #[test]
    fn edge_target_exists_rule_both_missing() {
        let mut g = minimal_graph();
        g.edges
            .push(Edge::new("nonexistent_source", "nonexistent_target"));
        let rule = Rule;
        let d = rule.apply(&g);
        assert_eq!(d.len(), 2);
        assert_eq!(d[0].severity, Severity::Error);
        assert_eq!(d[1].severity, Severity::Error);
    }

    // --- reachability: multiple unreachable nodes ---

    #[test]
    fn edge_target_exists_rule_no_edges() {
        let mut g = minimal_graph();
        g.edges.clear();
        let rule = Rule;
        let d = rule.apply(&g);
        assert!(d.is_empty());
    }

    // --- goal_gate_has_retry: goal_gate=false explicitly ---
}
