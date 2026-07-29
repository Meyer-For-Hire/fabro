use fabro_graphviz::graph::{self, Graph, Node};
use fabro_types::StageHandler;

use crate::{Diagnostic, LintRule, Severity};

pub(super) fn rule() -> Box<dyn LintRule> {
    Box::new(Rule)
}

/// Attributes that only specific handlers read, paired with the handlers that
/// consume them. On every other node type the attribute is inert: accepted by
/// the parser and read by nothing at runtime.
///
/// Node types are compared after canonicalization through
/// [`StageHandler::from_handler_type`], so alias types (`tool` runs the
/// command handler) accept the same attributes as their canonical form.
///
/// Attributes read by several handlers (`timeout`), resolved for every node
/// (`fidelity`, `retry_policy`, `max_visits`, `goal_gate`), or injectable via
/// model stylesheets (`model`, `provider`, `reasoning_effort`, `speed`,
/// `backend`) are deliberately not listed.
const HANDLER_SPECIFIC_ATTRS: &[(&str, &[StageHandler])] = &[
    ("script", &[StageHandler::Command]),
    ("language", &[StageHandler::Command]),
    ("stdin_source", &[StageHandler::Command]),
    ("duration", &[StageHandler::Wait]),
    ("max_parallel", &[StageHandler::Parallel]),
    ("output_retries", &[
        StageHandler::Agent,
        StageHandler::Prompt,
    ]),
    ("output_schema", &[
        StageHandler::Agent,
        StageHandler::Prompt,
        StageHandler::Command,
    ]),
    ("prompt", &[
        StageHandler::Agent,
        StageHandler::Prompt,
        StageHandler::ParallelFanIn,
    ]),
    ("review_target", &[StageHandler::Human]),
];

const SCRIPT_PROMPT_CONFLICT_RULE: &str = "script_prompt_conflict";

struct Rule;

fn script_prompt_conflict(node: &Node) -> Diagnostic {
    Diagnostic {
        rule: SCRIPT_PROMPT_CONFLICT_RULE.to_string(),
        severity: Severity::Error,
        message: format!(
            "Node '{}' sets both 'script' and 'prompt'. No built-in handler reads both: command \
             handlers consume 'script', while LLM handlers consume 'prompt'",
            node.id
        ),
        node_id: Some(node.id.clone()),
        edge: None,
        fix: Some(
            "Remove whichever attribute is wrong, or split the node into a command node and an \
             agent node"
                .to_string(),
        ),
        ..Diagnostic::default()
    }
}

impl LintRule for Rule {
    fn name(&self) -> &'static str {
        "inert_attribute"
    }

    fn apply(&self, graph: &Graph) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for node in graph.nodes.values() {
            let has_script_prompt_conflict =
                node.attrs.contains_key("script") && node.attrs.contains_key("prompt");
            if has_script_prompt_conflict {
                diagnostics.push(script_prompt_conflict(node));
            }

            // An unknown shape or type is covered by the type_known rule; a
            // node this rule cannot classify is skipped rather than guessed at.
            let Some(raw_type) = node.handler_type() else {
                continue;
            };
            if !graph::is_known_handler_type(raw_type) {
                continue;
            }
            let handler = StageHandler::from_handler_type(Some(raw_type));
            for (attr, consumers) in HANDLER_SPECIFIC_ATTRS {
                if !node.attrs.contains_key(*attr) {
                    continue;
                }
                if has_script_prompt_conflict && matches!(*attr, "script" | "prompt") {
                    continue;
                }
                if consumers.contains(&handler) {
                    continue;
                }
                let consumer_names = consumers
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                diagnostics.push(Diagnostic {
                    rule: self.name().to_string(),
                    severity: Severity::Warning,
                    message: format!(
                        "Node '{}' (type '{raw_type}') sets '{attr}', which is only read by {consumer_names} nodes and has no effect here",
                        node.id,
                    ),
                    node_id: Some(node.id.clone()),
                    edge: None,
                    fix: Some(format!(
                        "Remove '{attr}' or change the node to a type that reads it ({consumer_names})",
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
    use fabro_graphviz::graph::{AttrValue, Edge, Node};

    use super::Rule;
    use crate::rules::test_support;
    use crate::{LintRule, Severity};

    fn node_with_attr(id: &str, shape: &str, attr: &str, value: &str) -> Node {
        test_support::node_with_attrs(id, &[("shape", shape), (attr, value)])
    }

    #[test]
    fn warns_on_script_on_agent_node() {
        let mut g = test_support::minimal_graph();
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
    fn built_in_rules_report_script_prompt_conflict_once() {
        let mut g = test_support::minimal_graph();
        g.nodes.insert(
            "work".to_string(),
            test_support::node_with_attrs("work", &[
                ("script", "cargo build"),
                ("prompt", "do it"),
            ]),
        );
        g.edges = vec![Edge::new("start", "work"), Edge::new("work", "exit")];

        let diagnostics = crate::validate(&g, &[]);
        let work_diagnostics = diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.node_id.as_deref() == Some("work"))
            .collect::<Vec<_>>();

        assert_eq!(work_diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
        assert_eq!(work_diagnostics[0].rule, "script_prompt_conflict");
        assert_eq!(work_diagnostics[0].severity, Severity::Error);
    }

    #[test]
    fn conflict_error_replaces_inert_warning_for_explicit_shapes() {
        let mut g = test_support::minimal_graph();
        g.nodes.insert(
            "run".to_string(),
            test_support::node_with_attrs("run", &[
                ("shape", "parallelogram"),
                ("script", "cargo build"),
                ("prompt", "do it"),
            ]),
        );
        g.nodes.insert(
            "plan".to_string(),
            test_support::node_with_attrs("plan", &[
                ("shape", "box"),
                ("script", "cargo build"),
                ("prompt", "do it"),
            ]),
        );

        let diagnostics = Rule.apply(&g);

        assert_eq!(diagnostics.len(), 2);
        assert!(
            diagnostics
                .iter()
                .all(|diagnostic| diagnostic.rule == "script_prompt_conflict"
                    && diagnostic.severity == Severity::Error)
        );
    }

    #[test]
    fn conflict_uses_attribute_presence() {
        let mut g = test_support::minimal_graph();
        let mut node = test_support::node_with_attrs("work", &[("prompt", "do it")]);
        node.attrs
            .insert("script".to_string(), AttrValue::Integer(123));
        g.nodes.insert("work".to_string(), node);

        let diagnostics = Rule.apply(&g);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].rule, "script_prompt_conflict");
    }

    #[test]
    fn warns_on_prompt_on_start_and_command_nodes() {
        let mut g = test_support::minimal_graph();
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
        let mut g = test_support::minimal_graph();
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
        let mut g = test_support::minimal_graph();
        let mut node = Node::new("work");
        node.attrs
            .insert("max_parallel".to_string(), AttrValue::Integer(4));
        g.nodes.insert("work".to_string(), node);
        let d = Rule.apply(&g);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn warns_on_output_retries_on_command_node() {
        let mut g = test_support::minimal_graph();
        g.nodes.insert(
            "run".to_string(),
            node_with_attr("run", "parallelogram", "output_retries", "2"),
        );

        let d = Rule.apply(&g);

        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("'output_retries'"));
        assert!(d[0].message.contains("agent, prompt"));
    }

    #[test]
    fn accepts_attrs_on_their_own_handler_types() {
        let mut g = test_support::minimal_graph();
        let mut run = node_with_attr("run", "parallelogram", "script", "echo hi");
        run.attrs.insert(
            "stdin_source".to_string(),
            AttrValue::String("context.parallel.results".to_string()),
        );
        g.nodes.insert("run".to_string(), run);
        g.nodes.insert(
            "audit".to_string(),
            node_with_attr("audit", "parallelogram", "output_schema", "routing"),
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
            "review".to_string(),
            node_with_attr("review", "box", "output_retries", "2"),
        );
        g.nodes.insert(
            "fork".to_string(),
            node_with_attr("fork", "component", "max_parallel", "4"),
        );
        g.nodes.insert(
            "spec".to_string(),
            node_with_attr("spec", "tab", "output_schema", "routing"),
        );
        g.nodes.insert(
            "prompt".to_string(),
            node_with_attr("prompt", "tab", "output_retries", "2"),
        );
        g.nodes.insert(
            "human".to_string(),
            node_with_attr("human", "hexagon", "review_target", "true"),
        );
        g.nodes.insert(
            "legacy_command".to_string(),
            test_support::node_with_attrs("legacy_command", &[
                ("type", "tool"),
                ("script", "echo legacy"),
            ]),
        );
        assert!(Rule.apply(&g).is_empty());
    }

    #[test]
    fn warns_on_review_target_on_non_human_node() {
        let mut g = test_support::minimal_graph();
        g.nodes.insert(
            "work".to_string(),
            node_with_attr("work", "box", "review_target", "true"),
        );

        let diagnostics = Rule.apply(&g);

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("'review_target'"));
        assert!(diagnostics[0].message.contains("human"));
    }

    #[test]
    fn accepts_prompt_on_shapeless_node_defaulting_to_agent() {
        let mut g = test_support::minimal_graph();
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
        let mut g = test_support::minimal_graph();
        g.nodes.insert(
            "merge".to_string(),
            node_with_attr("merge", "tripleoctagon", "prompt", "pick the best"),
        );
        assert!(Rule.apply(&g).is_empty());
    }

    #[test]
    fn ignores_unclassifiable_node_shapes() {
        let mut g = test_support::minimal_graph();
        g.nodes.insert(
            "odd".to_string(),
            node_with_attr("odd", "doubleoctagon", "script", "echo hi"),
        );
        assert!(Rule.apply(&g).is_empty());
    }

    #[test]
    fn ignores_handler_specific_attrs_on_unrecognized_explicit_types() {
        let mut g = test_support::minimal_graph();
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
