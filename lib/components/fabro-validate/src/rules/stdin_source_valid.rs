use fabro_graphviz::graph::Graph;

use crate::{Diagnostic, LintRule, Severity};

pub(super) fn rule() -> Box<dyn LintRule> {
    Box::new(Rule)
}

struct Rule;

impl LintRule for Rule {
    fn name(&self) -> &'static str {
        "stdin_source_valid"
    }

    fn apply(&self, graph: &Graph) -> Vec<Diagnostic> {
        graph
            .nodes
            .values()
            .filter(|node| matches!(node.handler_type(), Some("command" | "tool")))
            .filter(|node| node.attrs.contains_key("stdin_source"))
            .filter(|node| {
                node.stdin_source()
                    .is_none_or(|source| source.trim().is_empty())
            })
            .map(|node| Diagnostic {
                rule: self.name().to_string(),
                severity: Severity::Error,
                message: format!(
                    "Command node '{}' has an empty or non-string 'stdin_source'",
                    node.id
                ),
                node_id: Some(node.id.clone()),
                edge: None,
                fix: Some(
                    "Set 'stdin_source' to a context key such as \"context.parallel.results\""
                        .to_string(),
                ),
                ..Diagnostic::default()
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use fabro_graphviz::graph::{AttrValue, Node};

    use super::Rule;
    use crate::rules::test_support::minimal_graph;
    use crate::{LintRule, Severity};

    fn command_node(value: AttrValue) -> Node {
        let mut node = Node::new("merge");
        node.attrs.insert(
            "shape".to_string(),
            AttrValue::String("parallelogram".to_string()),
        );
        node.attrs.insert("stdin_source".to_string(), value);
        node
    }

    #[test]
    fn accepts_non_empty_context_source() {
        let mut graph = minimal_graph();
        graph.nodes.insert(
            "merge".to_string(),
            command_node(AttrValue::String("context.parallel.results".to_string())),
        );

        assert!(Rule.apply(&graph).is_empty());
    }

    #[test]
    fn rejects_empty_or_non_string_source() {
        for value in [
            AttrValue::String(String::new()),
            AttrValue::String("   ".to_string()),
            AttrValue::Integer(3),
        ] {
            let mut graph = minimal_graph();
            graph.nodes.insert("merge".to_string(), command_node(value));

            let diagnostics = Rule.apply(&graph);

            assert_eq!(diagnostics.len(), 1);
            assert_eq!(diagnostics[0].severity, Severity::Error);
            assert!(diagnostics[0].message.contains("empty or non-string"));
        }
    }
}
