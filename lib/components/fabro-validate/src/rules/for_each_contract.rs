use fabro_graphviz::graph::{Graph, is_llm_handler_type};

use crate::{Diagnostic, LintRule, Severity};

pub(super) fn rule() -> Box<dyn LintRule> {
    Box::new(Rule)
}

struct Rule;

fn diagnostic(node_id: &str, message: String, fix: impl Into<String>) -> Diagnostic {
    Diagnostic {
        rule: "for_each_contract".to_string(),
        severity: Severity::Error,
        message,
        node_id: Some(node_id.to_string()),
        edge: None,
        fix: Some(fix.into()),
        ..Diagnostic::default()
    }
}

impl LintRule for Rule {
    fn name(&self) -> &'static str {
        "for_each_contract"
    }

    fn apply(&self, graph: &Graph) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for node in graph.nodes.values() {
            if !node.attrs.contains_key("for_each") {
                continue;
            }

            // Reported as an error rather than through `inert_attribute`: an
            // ignored tuning knob is a warning, but a fan-out that silently
            // never happens is not.
            if node.handler_type() != Some("parallel") {
                diagnostics.push(diagnostic(
                    &node.id,
                    format!(
                        "Node '{}' sets 'for_each', but only parallel nodes can fan out over runtime items",
                        node.id
                    ),
                    "Remove 'for_each' or change the node to type=\"parallel\"",
                ));
                continue;
            }

            if node
                .for_each()
                .is_none_or(|source| source.trim().is_empty())
            {
                diagnostics.push(diagnostic(
                    &node.id,
                    format!(
                        "Node '{}' has an empty or non-string 'for_each' source",
                        node.id
                    ),
                    "Set 'for_each' to a context key such as \"context.candidates\"",
                ));
            }

            let outgoing = graph.outgoing_edges(&node.id);
            if outgoing.len() != 1 {
                diagnostics.push(diagnostic(
                    &node.id,
                    format!(
                        "Parallel node '{}' sets 'for_each' and must have exactly one outgoing template edge, but has {}",
                        node.id,
                        outgoing.len()
                    ),
                    "Keep one outgoing edge whose target is the agent or prompt to run for each item",
                ));
                continue;
            }

            let target_id = &outgoing[0].to;
            let Some(target) = graph.nodes.get(target_id) else {
                continue;
            };
            if target.attrs.contains_key("for_each") {
                diagnostics.push(diagnostic(
                    &node.id,
                    format!(
                        "Parallel node '{}' targets '{}', which also sets 'for_each'; nested for_each is not supported",
                        node.id, target.id
                    ),
                    "Remove the nested 'for_each' and use a single runtime fan-out",
                ));
            }
            if !is_llm_handler_type(target.handler_type()) {
                diagnostics.push(diagnostic(
                    &node.id,
                    format!(
                        "Parallel node '{}' sets 'for_each', but template target '{}' is not an agent or prompt node",
                        node.id, target.id
                    ),
                    "Target one agent or prompt node from the for_each parallel node",
                ));
            }
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use fabro_graphviz::graph::{AttrValue, Edge, Node};

    use super::Rule;
    use crate::rules::test_support::minimal_graph;
    use crate::{LintRule, Severity};

    fn for_each_node(id: &str) -> Node {
        let mut node = Node::new(id);
        node.attrs.insert(
            "type".to_string(),
            AttrValue::String("parallel".to_string()),
        );
        node.attrs.insert(
            "for_each".to_string(),
            AttrValue::String("context.items".to_string()),
        );
        node
    }

    #[test]
    fn accepts_one_agent_template_target() {
        let mut graph = minimal_graph();
        graph
            .nodes
            .insert("fanout".to_string(), for_each_node("fanout"));
        graph
            .nodes
            .insert("worker".to_string(), Node::new("worker"));
        graph.edges.push(Edge::new("fanout", "worker"));

        assert!(Rule.apply(&graph).is_empty());
    }

    #[test]
    fn rejects_for_each_on_non_parallel_node() {
        let mut graph = minimal_graph();
        let mut worker = Node::new("worker");
        worker.attrs.insert(
            "for_each".to_string(),
            AttrValue::String("items".to_string()),
        );
        graph.nodes.insert("worker".to_string(), worker);

        let diagnostics = Rule.apply(&graph);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, Severity::Error);
        assert!(diagnostics[0].message.contains("only parallel"));
    }

    #[test]
    fn rejects_multiple_template_edges() {
        let mut graph = minimal_graph();
        graph
            .nodes
            .insert("fanout".to_string(), for_each_node("fanout"));
        graph.nodes.insert("one".to_string(), Node::new("one"));
        graph.nodes.insert("two".to_string(), Node::new("two"));
        graph.edges.push(Edge::new("fanout", "one"));
        graph.edges.push(Edge::new("fanout", "two"));

        let diagnostics = Rule.apply(&graph);

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("exactly one"));
    }

    #[test]
    fn rejects_non_llm_and_nested_template_targets() {
        let mut graph = minimal_graph();
        graph
            .nodes
            .insert("outer".to_string(), for_each_node("outer"));
        graph
            .nodes
            .insert("inner".to_string(), for_each_node("inner"));
        graph.edges.push(Edge::new("outer", "inner"));

        let diagnostics = Rule.apply(&graph);
        let outer_diagnostics = diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.node_id.as_deref() == Some("outer"))
            .collect::<Vec<_>>();

        assert_eq!(outer_diagnostics.len(), 2);
        assert!(diagnostics.iter().all(|d| d.severity == Severity::Error));
        assert!(
            outer_diagnostics
                .iter()
                .any(|d| d.message.contains("nested"))
        );
        assert!(
            outer_diagnostics
                .iter()
                .any(|d| d.message.contains("not an agent or prompt"))
        );
    }

    #[test]
    fn rejects_empty_or_non_string_source() {
        for value in [
            AttrValue::String(String::new()),
            AttrValue::String("   ".to_string()),
            AttrValue::Integer(3),
        ] {
            let mut graph = minimal_graph();
            let mut fanout = for_each_node("fanout");
            fanout.attrs.insert("for_each".to_string(), value);
            graph.nodes.insert("fanout".to_string(), fanout);
            graph
                .nodes
                .insert("worker".to_string(), Node::new("worker"));
            graph.edges.push(Edge::new("fanout", "worker"));

            let diagnostics = Rule.apply(&graph);

            assert_eq!(diagnostics.len(), 1);
            assert!(diagnostics[0].message.contains("empty or non-string"));
        }
    }
}
