use fabro_graphviz::graph::{self, Graph};

use crate::{Diagnostic, LintRule, Severity};

pub(super) fn rule() -> Box<dyn LintRule> {
    Box::new(Rule)
}

/// Attributes that only specific handler types read, paired with the handler
/// types that consume them. On every other node type the attribute is inert:
/// accepted by the parser and read by nothing at runtime.
///
/// Attributes read by several handlers (`timeout`), resolved for every node
/// (`fidelity`, `retry_policy`, `max_visits`, `goal_gate`), or injectable via
/// model stylesheets (`model`, `provider`, `reasoning_effort`, `speed`,
/// `backend`) are deliberately not listed.
const HANDLER_SPECIFIC_ATTRS: &[(&str, &[&str])] = &[
    ("script", &["command"]),
    ("language", &["command"]),
    ("duration", &["wait"]),
    ("max_parallel", &["parallel"]),
    ("output_schema", &["agent", "prompt"]),
    ("prompt", &["agent", "prompt", "parallel.fan_in"]),
];

struct Rule;

impl LintRule for Rule {
    fn name(&self) -> &'static str {
        "inert_attribute"
    }

    fn apply(&self, graph: &Graph) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for node in graph.nodes.values() {
            // An unknown shape or type is covered by the type_known rule; a
            // node this rule cannot classify is skipped rather than guessed at.
            let Some(handler) = node.handler_type() else {
                continue;
            };
            if !graph::is_known_handler_type(handler) {
                continue;
            }
            for (attr, consumers) in HANDLER_SPECIFIC_ATTRS {
                if !node.attrs.contains_key(*attr) {
                    continue;
                }
                if consumers.contains(&handler) {
                    continue;
                }
                diagnostics.push(Diagnostic {
                    rule: self.name().to_string(),
                    severity: Severity::Warning,
                    message: format!(
                        "Node '{}' (type '{handler}') sets '{attr}', which is only read by {} nodes and has no effect here",
                        node.id,
                        consumers.join(", "),
                    ),
                    node_id: Some(node.id.clone()),
                    edge: None,
                    fix: Some(format!(
                        "Remove '{attr}' or change the node to a type that reads it ({})",
                        consumers.join(", "),
                    )),
                    ..Diagnostic::default()
                });
            }
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

    fn node_with_attr(id: &str, shape: &str, attr: &str, value: &str) -> Node {
        let mut node = Node::new(id);
        node.attrs
            .insert("shape".to_string(), AttrValue::String(shape.to_string()));
        node.attrs
            .insert(attr.to_string(), AttrValue::String(value.to_string()));
        node
    }

    #[test]
    fn warns_on_script_on_agent_node() {
        let mut g = minimal_graph();
        g.nodes.insert(
            "work".to_string(),
            node_with_attr("work", "box", "script", "echo hi"),
        );
        let d = Rule.apply(&g);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].severity, Severity::Warning);
        assert!(d[0].message.contains("'script'"));
        assert!(d[0].message.contains("command"));
        assert_eq!(d[0].node_id.as_deref(), Some("work"));
    }

    #[test]
    fn warns_on_prompt_on_start_and_command_nodes() {
        let mut g = minimal_graph();
        g.nodes
            .get_mut("start")
            .expect("minimal graph has start")
            .attrs
            .insert(
                "prompt".to_string(),
                AttrValue::String("do things".to_string()),
            );
        g.nodes.insert(
            "run".to_string(),
            node_with_attr("run", "parallelogram", "prompt", "do things"),
        );
        let d = Rule.apply(&g);
        assert_eq!(d.len(), 2);
        assert!(d.iter().all(|d| d.message.contains("'prompt'")));
    }

    #[test]
    fn warns_on_duration_on_command_node() {
        let mut g = minimal_graph();
        g.nodes.insert(
            "run".to_string(),
            node_with_attr("run", "parallelogram", "duration", "30s"),
        );
        let d = Rule.apply(&g);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("'duration'"));
        assert!(d[0].message.contains("wait"));
    }

    #[test]
    fn warns_on_parallel_attrs_on_agent_node() {
        let mut g = minimal_graph();
        let mut node = Node::new("work");
        node.attrs
            .insert("max_parallel".to_string(), AttrValue::Integer(4));
        g.nodes.insert("work".to_string(), node);
        let d = Rule.apply(&g);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn warns_on_output_schema_on_command_node() {
        let mut g = minimal_graph();
        g.nodes.insert(
            "run".to_string(),
            node_with_attr("run", "parallelogram", "output_schema", "routing"),
        );
        let d = Rule.apply(&g);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("'output_schema'"));
    }

    #[test]
    fn accepts_attrs_on_their_own_handler_types() {
        let mut g = minimal_graph();
        g.nodes.insert(
            "run".to_string(),
            node_with_attr("run", "parallelogram", "script", "echo hi"),
        );
        g.nodes.insert(
            "pause".to_string(),
            node_with_attr("pause", "insulator", "duration", "30s"),
        );
        g.nodes.insert(
            "work".to_string(),
            node_with_attr("work", "box", "prompt", "do things"),
        );
        g.nodes.insert(
            "fork".to_string(),
            node_with_attr("fork", "component", "max_parallel", "4"),
        );
        g.nodes.insert(
            "spec".to_string(),
            node_with_attr("spec", "tab", "output_schema", "routing"),
        );
        assert!(Rule.apply(&g).is_empty());
    }

    #[test]
    fn accepts_prompt_on_shapeless_node_defaulting_to_agent() {
        let mut g = minimal_graph();
        let mut node = Node::new("work");
        node.attrs.insert(
            "prompt".to_string(),
            AttrValue::String("do things".to_string()),
        );
        g.nodes.insert("work".to_string(), node);
        assert!(Rule.apply(&g).is_empty());
    }

    #[test]
    fn accepts_prompt_on_fan_in_judge() {
        let mut g = minimal_graph();
        g.nodes.insert(
            "merge".to_string(),
            node_with_attr("merge", "tripleoctagon", "prompt", "pick the best"),
        );
        assert!(Rule.apply(&g).is_empty());
    }

    #[test]
    fn ignores_unclassifiable_node_shapes() {
        let mut g = minimal_graph();
        g.nodes.insert(
            "odd".to_string(),
            node_with_attr("odd", "doubleoctagon", "script", "echo hi"),
        );
        assert!(Rule.apply(&g).is_empty());
    }

    #[test]
    fn ignores_handler_specific_attrs_on_unrecognized_explicit_types() {
        let mut g = minimal_graph();
        let mut node = Node::new("custom");
        node.attrs.insert(
            "type".to_string(),
            AttrValue::String("custom.handler".to_string()),
        );
        node.attrs.insert(
            "script".to_string(),
            AttrValue::String("echo hi".to_string()),
        );
        g.nodes.insert("custom".to_string(), node);
        assert!(Rule.apply(&g).is_empty());
    }
}
