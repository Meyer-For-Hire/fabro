use fabro_graphviz::graph::Graph;

use crate::{Diagnostic, LintRule, Severity};

pub(super) fn rule() -> Box<dyn LintRule> {
    Box::new(Rule)
}

struct Rule;

impl LintRule for Rule {
    fn name(&self) -> &'static str {
        "join_policy_removed"
    }

    fn apply(&self, graph: &Graph) -> Vec<Diagnostic> {
        graph
            .nodes
            .values()
            .filter(|node| node.attrs.contains_key("join_policy"))
            .map(|node| Diagnostic {
                rule: self.name().to_string(),
                severity: Severity::Error,
                message: format!(
                    "Node '{}' sets the removed 'join_policy' attribute. Remove 'join_policy'; parallel nodes always wait for every branch to finish",
                    node.id,
                ),
                node_id: Some(node.id.clone()),
                edge: None,
                fix: Some("Remove 'join_policy' from this node".to_string()),
                ..Diagnostic::default()
            })
            .collect()
    }
}
