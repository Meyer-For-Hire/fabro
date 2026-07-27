use fabro_graphviz::graph::Graph;

use crate::{Diagnostic, LintRule, Severity};

pub(super) fn rule() -> Box<dyn LintRule> {
    Box::new(Rule)
}

struct Rule;

impl LintRule for Rule {
    fn name(&self) -> &'static str {
        "script_prompt_conflict"
    }

    fn apply(&self, graph: &Graph) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for node in graph.nodes.values() {
            if node.script().is_none() || node.prompt().is_none() {
                continue;
            }
            diagnostics.push(Diagnostic {
                rule: self.name().to_string(),
                severity: Severity::Error,
                message: format!(
                    "Node '{}' sets both 'script' and 'prompt'. No node type reads both: \
                     'script' selects the command handler and 'prompt' selects an LLM handler",
                    node.id
                ),
                node_id: Some(node.id.clone()),
                edge: None,
                fix: Some(
                    "Remove whichever attribute is wrong, or split the node into a command node \
                     and an agent node"
                        .to_string(),
                ),
                ..Diagnostic::default()
            });
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use fabro_graphviz::graph::{AttrValue, Node};

    use super::Rule;
    use crate::rules::test_support::minimal_graph;
    use crate::{LintRule, Severity};

    fn node_with(id: &str, attrs: &[(&str, &str)]) -> Node {
        let mut node = Node::new(id);
        for (key, value) in attrs {
            node.attrs
                .insert((*key).to_string(), AttrValue::String((*value).to_string()));
        }
        node
    }

    #[test]
    fn errors_when_shapeless_node_sets_both() {
        let mut g = minimal_graph();
        g.nodes.insert(
            "work".to_string(),
            node_with("work", &[("script", "cargo build"), ("prompt", "do it")]),
        );

        let d = Rule.apply(&g);

        assert_eq!(d.len(), 1);
        assert_eq!(d[0].severity, Severity::Error);
        assert!(d[0].message.contains("'script'"));
        assert!(d[0].message.contains("'prompt'"));
        assert_eq!(d[0].node_id.as_deref(), Some("work"));
    }

    #[test]
    fn errors_even_when_shape_is_explicit() {
        // An explicit shape resolves the handler, but the unread attribute is
        // still a mistake, and reporting it the same way everywhere keeps
        // adding a shape from turning an error into a warning.
        let mut g = minimal_graph();
        g.nodes.insert(
            "run".to_string(),
            node_with("run", &[
                ("shape", "parallelogram"),
                ("script", "cargo build"),
                ("prompt", "do it"),
            ]),
        );
        g.nodes.insert(
            "plan".to_string(),
            node_with("plan", &[
                ("shape", "box"),
                ("script", "cargo build"),
                ("prompt", "do it"),
            ]),
        );

        let d = Rule.apply(&g);

        assert_eq!(d.len(), 2);
        assert!(d.iter().all(|d| d.severity == Severity::Error));
    }

    #[test]
    fn accepts_either_attribute_alone() {
        let mut g = minimal_graph();
        g.nodes.insert(
            "build".to_string(),
            node_with("build", &[("script", "cargo build")]),
        );
        g.nodes.insert(
            "plan".to_string(),
            node_with("plan", &[("prompt", "do it")]),
        );

        assert!(Rule.apply(&g).is_empty());
    }

    #[test]
    fn ignores_legacy_tool_command_attribute() {
        let mut g = minimal_graph();
        g.nodes.insert(
            "work".to_string(),
            node_with("work", &[
                ("tool_command", "cargo build"),
                ("prompt", "do it"),
            ]),
        );

        assert!(Rule.apply(&g).is_empty());
    }
}
