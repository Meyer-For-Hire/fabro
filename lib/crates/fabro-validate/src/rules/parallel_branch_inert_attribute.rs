use fabro_graphviz::graph::Graph;

use super::parallel_branch::ParallelBranches;
use crate::{Diagnostic, LintRule, Severity};

pub(super) fn rule() -> Box<dyn LintRule> {
    Box::new(Rule)
}

/// Attributes that parallel branch execution does not resolve. Per-branch
/// preambles now honor fidelity, while `thread_id` remains inert because
/// concurrent branches cannot share an LLM session.
const BRANCH_IGNORED_ATTRS: &[&str] = &["thread_id"];

struct Rule;

/// Renders one or more parallel-node ids as `'a'` or `'a', 'b'`.
fn quoted_list(ids: &[String]) -> String {
    ids.iter()
        .map(|id| format!("'{id}'"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn fix_message(attr: &str) -> String {
    match attr {
        "thread_id" => format!(
            "Remove '{attr}': parallel branches inherit the thread resolved when the parallel node started"
        ),
        _ => format!("Remove '{attr}'"),
    }
}

fn full_fidelity_message() -> String {
    "parallel branches run at most at summary:high; full is degraded at runtime because branches cannot share a session".to_string()
}

fn full_fidelity_fix(parallel_ids: &[String]) -> String {
    let parent = if parallel_ids.len() == 1 {
        format!("parallel node {}", quoted_list(parallel_ids))
    } else {
        format!("parallel nodes {}", quoted_list(parallel_ids))
    };
    format!(
        "Use fidelity=\"summary:high\" or another lower mode on this branch; to reuse a full session before fan-out, set fidelity=\"full\" on {parent} or its incoming edge"
    )
}

impl LintRule for Rule {
    fn name(&self) -> &'static str {
        "parallel_branch_inert_attribute"
    }

    fn apply(&self, graph: &Graph) -> Vec<Diagnostic> {
        let branches = ParallelBranches::new(graph);
        if branches.is_empty() {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();

        for edge in &graph.edges {
            if !branches.is_fork_edge(edge) {
                continue;
            }
            if edge.fidelity() == Some("full") {
                diagnostics.push(Diagnostic {
                    rule: self.name().to_string(),
                    severity: Severity::Warning,
                    message: full_fidelity_message(),
                    node_id: None,
                    edge: Some((edge.from.clone(), edge.to.clone())),
                    fix: Some(full_fidelity_fix(std::slice::from_ref(&edge.from))),
                    ..Diagnostic::default()
                });
            }
            for attr in BRANCH_IGNORED_ATTRS {
                if !edge.attrs.contains_key(*attr) {
                    continue;
                }
                diagnostics.push(Diagnostic {
                    rule: self.name().to_string(),
                    severity: Severity::Warning,
                    message: format!(
                        "Edge {} -> {} sets '{attr}', which is ignored on parallel branch edges: branches receive the context snapshot taken when '{}' started",
                        edge.from, edge.to, edge.from,
                    ),
                    node_id: None,
                    edge: Some((edge.from.clone(), edge.to.clone())),
                    fix: Some(fix_message(attr)),
                    ..Diagnostic::default()
                });
            }
        }

        // A node with any normal incoming path still resolves its attributes on
        // that path, so branch-only diagnostics do not apply to it.
        for target in branches.branch_targets() {
            let Some(parents) = branches.branch_only_parents(target) else {
                continue;
            };
            let Some(node) = graph.nodes.get(target) else {
                continue;
            };
            if node.fidelity() == Some("full") {
                diagnostics.push(Diagnostic {
                    rule: self.name().to_string(),
                    severity: Severity::Warning,
                    message: full_fidelity_message(),
                    node_id: Some(node.id.clone()),
                    edge: None,
                    fix: Some(full_fidelity_fix(&parents)),
                    ..Diagnostic::default()
                });
            }
            for attr in BRANCH_IGNORED_ATTRS {
                if !node.attrs.contains_key(*attr) {
                    continue;
                }
                diagnostics.push(Diagnostic {
                    rule: self.name().to_string(),
                    severity: Severity::Warning,
                    message: format!(
                        "Node '{}' sets '{attr}', but it only runs as a parallel branch (of {}), where '{attr}' is ignored: branches receive the context snapshot taken when the parallel node started",
                        node.id,
                        quoted_list(&parents),
                    ),
                    node_id: Some(node.id.clone()),
                    edge: None,
                    fix: Some(fix_message(attr)),
                    ..Diagnostic::default()
                });
            }
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

    fn shaped_node(id: &str, shape: &str) -> Node {
        let mut node = Node::new(id);
        node.attrs
            .insert("shape".to_string(), AttrValue::String(shape.to_string()));
        node
    }

    /// start -> fork -> {branch_a, branch_b} -> merge -> exit
    fn parallel_graph() -> Graph {
        let mut g = minimal_graph();
        g.nodes
            .insert("fork".to_string(), shaped_node("fork", "component"));
        g.nodes
            .insert("branch_a".to_string(), shaped_node("branch_a", "tab"));
        g.nodes
            .insert("branch_b".to_string(), shaped_node("branch_b", "tab"));
        g.nodes
            .insert("merge".to_string(), shaped_node("merge", "tripleoctagon"));
        g.edges = vec![
            Edge::new("start", "fork"),
            Edge::new("fork", "branch_a"),
            Edge::new("fork", "branch_b"),
            Edge::new("branch_a", "merge"),
            Edge::new("branch_b", "merge"),
            Edge::new("merge", "exit"),
        ];
        g
    }

    #[test]
    fn accepts_non_full_fidelity_on_branch_node() {
        let mut g = parallel_graph();
        g.nodes
            .get_mut("branch_a")
            .expect("graph has branch_a")
            .attrs
            .insert(
                "fidelity".to_string(),
                AttrValue::String("truncate".to_string()),
            );

        assert!(Rule.apply(&g).is_empty());
    }

    #[test]
    fn warns_when_full_fidelity_on_branch_node_degrades() {
        let mut g = parallel_graph();
        g.nodes
            .get_mut("branch_a")
            .expect("graph has branch_a")
            .attrs
            .insert(
                "fidelity".to_string(),
                AttrValue::String("full".to_string()),
            );

        let d = Rule.apply(&g);

        assert_eq!(d.len(), 1);
        assert_eq!(d[0].severity, Severity::Warning);
        assert_eq!(d[0].node_id.as_deref(), Some("branch_a"));
        assert!(d[0].message.contains("full"));
        assert!(d[0].message.contains("summary:high"));
        assert!(d[0].fix.as_deref().is_some_and(|f| f.contains("'fork'")));
    }

    #[test]
    fn accepts_every_non_full_fidelity_on_branch_edges() {
        for fidelity in [
            "truncate",
            "compact",
            "summary:low",
            "summary:medium",
            "summary:high",
        ] {
            let mut g = parallel_graph();
            g.edges[1].attrs.insert(
                "fidelity".to_string(),
                AttrValue::String(fidelity.to_string()),
            );

            assert!(
                Rule.apply(&g).is_empty(),
                "{fidelity} should be accepted on a branch edge"
            );
        }
    }

    #[test]
    fn warns_when_full_fidelity_on_branch_edge_degrades() {
        let mut g = parallel_graph();
        g.edges[1].attrs.insert(
            "fidelity".to_string(),
            AttrValue::String("full".to_string()),
        );

        let d = Rule.apply(&g);

        assert_eq!(d.len(), 1);
        assert_eq!(
            d[0].edge,
            Some(("fork".to_string(), "branch_a".to_string()))
        );
        assert!(d[0].message.contains("full"));
        assert!(d[0].message.contains("summary:high"));
    }

    #[test]
    fn warns_on_thread_id_on_branch_edge() {
        let mut g = parallel_graph();
        g.edges[1].attrs.insert(
            "thread_id".to_string(),
            AttrValue::String("impl".to_string()),
        );
        let d = Rule.apply(&g);
        assert_eq!(d.len(), 1);
        assert_eq!(
            d[0].edge,
            Some(("fork".to_string(), "branch_a".to_string()))
        );
        assert!(d[0].message.contains("'thread_id'"));
        assert_eq!(
            d[0].fix.as_deref(),
            Some(
                "Remove 'thread_id': parallel branches inherit the thread resolved when the parallel node started"
            )
        );
    }

    #[test]
    fn warns_on_thread_id_on_branch_only_node() {
        let mut g = parallel_graph();
        g.nodes
            .get_mut("branch_a")
            .expect("graph has branch_a")
            .attrs
            .insert(
                "thread_id".to_string(),
                AttrValue::String("impl".to_string()),
            );

        let d = Rule.apply(&g);

        assert_eq!(d.len(), 1);
        assert_eq!(d[0].node_id.as_deref(), Some("branch_a"));
        assert!(d[0].message.contains("'thread_id'"));
        assert_eq!(
            d[0].fix.as_deref(),
            Some(
                "Remove 'thread_id': parallel branches inherit the thread resolved when the parallel node started"
            )
        );
    }

    #[test]
    fn accepts_fidelity_on_the_parallel_node_itself() {
        let mut g = parallel_graph();
        g.nodes
            .get_mut("fork")
            .expect("graph has fork")
            .attrs
            .insert(
                "fidelity".to_string(),
                AttrValue::String("truncate".to_string()),
            );
        assert!(Rule.apply(&g).is_empty());
    }

    #[test]
    fn accepts_fidelity_on_branch_node_also_reached_by_normal_edge() {
        let mut g = parallel_graph();
        // branch_a is also a normal successor of merge, so fidelity resolves
        // on that path and is not inert.
        g.edges.push(Edge::new("merge", "branch_a"));
        g.nodes
            .get_mut("branch_a")
            .expect("graph has branch_a")
            .attrs
            .insert(
                "fidelity".to_string(),
                AttrValue::String("truncate".to_string()),
            );
        assert!(Rule.apply(&g).is_empty());
    }

    #[test]
    fn names_every_parallel_parent_of_a_shared_branch_node() {
        let mut g = parallel_graph();
        g.nodes
            .insert("fork2".to_string(), shaped_node("fork2", "component"));
        g.edges.push(Edge::new("start", "fork2"));
        g.edges.push(Edge::new("fork2", "branch_a"));
        g.nodes
            .get_mut("branch_a")
            .expect("graph has branch_a")
            .attrs
            .insert(
                "fidelity".to_string(),
                AttrValue::String("full".to_string()),
            );
        let d = Rule.apply(&g);
        assert_eq!(d.len(), 1);
        let fix = d[0].fix.as_deref().expect("diagnostic has a fix");
        assert!(fix.contains("'fork', 'fork2'"));
        assert!(fix.contains("parallel nodes"));
    }

    #[test]
    fn accepts_graph_without_parallel_nodes() {
        let mut g = minimal_graph();
        let mut node = shaped_node("work", "tab");
        node.attrs.insert(
            "fidelity".to_string(),
            AttrValue::String("truncate".to_string()),
        );
        g.nodes.insert("work".to_string(), node);
        assert!(Rule.apply(&g).is_empty());
    }
}
