use fabro_graphviz::graph::Graph;

use crate::{Diagnostic, LintRule, Severity};

pub(super) fn rule() -> Box<dyn LintRule> {
    Box::new(Rule)
}

struct Rule;

impl LintRule for Rule {
    fn name(&self) -> &'static str {
        "command_requires_script"
    }

    fn apply(&self, graph: &Graph) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for node in graph.nodes.values() {
            if node.handler_type() != Some("command") {
                continue;
            }
            if node.script().is_some_and(|s| !s.trim().is_empty()) {
                continue;
            }
            diagnostics.push(Diagnostic {
                rule: self.name().to_string(),
                severity: Severity::Error,
                message: format!("Command node '{}' has no 'script' to run", node.id),
                node_id: Some(node.id.clone()),
                edge: None,
                fix: Some(
                    "Add a 'script' attribute, or remove the command shape or type if this was \
                     meant to be an agent node"
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
    fn errors_on_command_shape_without_script() {
        let mut g = minimal_graph();
        g.nodes.insert(
            "run".to_string(),
            node_with("run", &[("shape", "parallelogram"), ("label", "Build")]),
        );

        let d = Rule.apply(&g);

        assert_eq!(d.len(), 1);
        assert_eq!(d[0].severity, Severity::Error);
        assert!(d[0].message.contains("no 'script'"));
        assert_eq!(d[0].node_id.as_deref(), Some("run"));
    }

    #[test]
    fn errors_on_explicit_command_type_without_script() {
        let mut g = minimal_graph();
        g.nodes
            .insert("run".to_string(), node_with("run", &[("type", "command")]));

        let d = Rule.apply(&g);

        assert_eq!(d.len(), 1);
        assert_eq!(d[0].severity, Severity::Error);
    }

    #[test]
    fn errors_on_blank_script() {
        let mut g = minimal_graph();
        g.nodes.insert(
            "run".to_string(),
            node_with("run", &[("shape", "parallelogram"), ("script", "   ")]),
        );

        assert_eq!(Rule.apply(&g).len(), 1);
    }

    #[test]
    fn accepts_command_node_with_script() {
        let mut g = minimal_graph();
        g.nodes.insert(
            "run".to_string(),
            node_with("run", &[
                ("shape", "parallelogram"),
                ("script", "cargo build"),
            ]),
        );
        g.nodes.insert(
            "build".to_string(),
            node_with("build", &[("script", "cargo build")]),
        );

        assert!(Rule.apply(&g).is_empty());
    }

    #[test]
    fn ignores_non_command_nodes() {
        let mut g = minimal_graph();
        g.nodes.insert(
            "plan".to_string(),
            node_with("plan", &[("prompt", "do it")]),
        );
        g.nodes.insert(
            "gate".to_string(),
            node_with("gate", &[("shape", "hexagon")]),
        );

        assert!(Rule.apply(&g).is_empty());
    }
}
