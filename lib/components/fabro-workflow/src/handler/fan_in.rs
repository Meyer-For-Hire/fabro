use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use fabro_graphviz::graph::{Graph, Node};

use super::agent::CodergenBackend;
use super::prompt::PromptHandler;
use super::{EngineServices, Handler};
use crate::context::{Context, keys};
use crate::error::Error;
use crate::event::Emitter;
use crate::outcome::Outcome;

/// Joins results from a preceding parallel node.
///
/// Promptless fan-in nodes are barriers. Prompted fan-in nodes use the same
/// execution path as standard prompt stages and synthesize the full ordered
/// branch result set without selecting workspace state.
pub struct FanInHandler {
    prompt_handler: PromptHandler,
}

impl FanInHandler {
    #[must_use]
    pub fn new(backend: Option<Box<dyn CodergenBackend>>) -> Self {
        Self {
            prompt_handler: PromptHandler::new(backend),
        }
    }
}

impl FanInHandler {
    async fn run_join(
        &self,
        node: &Node,
        context: &Context,
        graph: &Graph,
        run_dir: &Path,
        services: &EngineServices,
        simulated: bool,
    ) -> Result<Outcome, Error> {
        let branch_count = validated_branch_count(context)?;
        if node
            .prompt()
            .is_some_and(|prompt| !prompt.trim().is_empty())
        {
            return if simulated {
                self.prompt_handler
                    .simulate(node, context, graph, run_dir, services)
                    .await
            } else {
                self.prompt_handler
                    .execute(node, context, graph, run_dir, services)
                    .await
            };
        }
        Ok(joined_outcome(branch_count, simulated))
    }
}

#[async_trait]
impl Handler for FanInHandler {
    async fn shutdown(&self, emitter: &Arc<Emitter>) {
        self.prompt_handler.shutdown(emitter).await;
    }

    async fn simulate(
        &self,
        node: &Node,
        context: &Context,
        graph: &Graph,
        run_dir: &Path,
        services: &EngineServices,
    ) -> Result<Outcome, Error> {
        self.run_join(node, context, graph, run_dir, services, true)
            .await
    }

    async fn execute(
        &self,
        node: &Node,
        context: &Context,
        graph: &Graph,
        run_dir: &Path,
        services: &EngineServices,
    ) -> Result<Outcome, Error> {
        self.run_join(node, context, graph, run_dir, services, false)
            .await
    }
}

/// Validate that `parallel.results` exists and has the typed shape without
/// cloning the (potentially hydrated) branch payloads into a full
/// [`ParallelBranchResult`] vec that would go unused.
fn validated_branch_count(context: &Context) -> Result<usize, Error> {
    #[derive(serde::Deserialize)]
    struct BranchShape {
        #[expect(dead_code, reason = "deserialized only to validate the shape")]
        id:     String,
        #[expect(dead_code, reason = "deserialized only to validate the shape")]
        status: fabro_types::StageOutcome,
    }

    let value = context
        .get(keys::PARALLEL_RESULTS)
        .ok_or_else(|| Error::handler("No parallel results to join"))?;
    let results: Vec<BranchShape> = serde_json::from_value(value)
        .map_err(|err| Error::handler_with_source("Invalid parallel results", err))?;
    Ok(results.len())
}

fn joined_outcome(branch_count: usize, simulated: bool) -> Outcome {
    let mut outcome = Outcome::success();
    let prefix = if simulated { "[Simulated] " } else { "" };
    outcome.notes = Some(format!(
        "{prefix}Joined {branch_count} parallel {}",
        if branch_count == 1 {
            "branch"
        } else {
            "branches"
        }
    ));
    outcome
}

#[cfg(test)]
mod tests {
    use fabro_graphviz::graph::AttrValue;
    use fabro_types::StageTiming;
    use tempfile::TempDir;

    use super::*;
    use crate::handler::agent::{CodergenResult, CodergenRunRequest, OneShotRequest};
    use crate::outcome::StageOutcome;

    fn make_services() -> EngineServices {
        EngineServices::test_default()
    }

    fn context_with_results() -> Context {
        let context = Context::new();
        context.set(
            keys::PARALLEL_RESULTS,
            serde_json::json!([
                {
                    "id": "branch_a",
                    "status": "failed",
                    "context_updates": {"command.output": "failure details"}
                },
                {
                    "id": "branch_b",
                    "status": "succeeded",
                    "context_updates": {"response.branch_b": "complete response"}
                }
            ]),
        );
        context
    }

    #[tokio::test]
    async fn promptless_fan_in_is_a_noop_barrier() {
        let outcome = FanInHandler::new(None)
            .execute(
                &Node::new("fan_in"),
                &context_with_results(),
                &Graph::new("test"),
                Path::new("/tmp/test"),
                &make_services(),
            )
            .await
            .unwrap();

        assert_eq!(outcome.status, StageOutcome::Succeeded);
        assert_eq!(outcome.notes.as_deref(), Some("Joined 2 parallel branches"));
        assert!(outcome.context_updates.is_empty());
    }

    #[tokio::test]
    async fn fan_in_requires_typed_parallel_results() {
        let context = Context::new();
        let missing = FanInHandler::new(None)
            .execute(
                &Node::new("fan_in"),
                &context,
                &Graph::new("test"),
                Path::new("/tmp/test"),
                &make_services(),
            )
            .await;
        assert!(missing.is_err());

        context.set(keys::PARALLEL_RESULTS, serde_json::json!([{"id": "a"}]));
        let invalid = FanInHandler::new(None)
            .execute(
                &Node::new("fan_in"),
                &context,
                &Graph::new("test"),
                Path::new("/tmp/test"),
                &make_services(),
            )
            .await;
        assert!(invalid.is_err());
    }

    #[tokio::test]
    async fn prompted_fan_in_uses_standard_prompt_response_fields() {
        struct ReducerBackend;

        #[async_trait]
        impl CodergenBackend for ReducerBackend {
            async fn run(&self, _request: CodergenRunRequest<'_>) -> Result<CodergenResult, Error> {
                panic!("prompted fan-in must use one_shot like a standard prompt")
            }

            async fn one_shot(&self, request: OneShotRequest<'_>) -> Result<CodergenResult, Error> {
                assert!(request.prompt.contains("Synthesize every result"));
                Ok(CodergenResult::Text {
                    text:              "combined result".to_string(),
                    usage:             None,
                    files_touched:     Vec::new(),
                    last_file_touched: None,
                    timing:            StageTiming::new(0, 20, 30),
                })
            }
        }

        let handler = FanInHandler::new(Some(Box::new(ReducerBackend)));
        let mut node = Node::new("fan_in");
        node.attrs.insert(
            "prompt".to_string(),
            AttrValue::String("Synthesize every result".to_string()),
        );
        let run_dir = TempDir::new().unwrap();
        let outcome = handler
            .execute(
                &node,
                &context_with_results(),
                &Graph::new("test"),
                run_dir.path(),
                &make_services(),
            )
            .await
            .unwrap();

        assert_eq!(outcome.status, StageOutcome::Succeeded);
        assert_eq!(
            outcome.context_updates.get(&keys::response_key("fan_in")),
            Some(&serde_json::json!("combined result"))
        );
        assert_eq!(
            outcome.context_updates.get(keys::LAST_RESPONSE),
            Some(&serde_json::json!("combined result"))
        );
        assert_eq!(outcome.timing, Some(StageTiming::new(0, 20, 30)));
        assert!(
            outcome
                .context_updates
                .keys()
                .all(|key| !key.starts_with("parallel.fan_in.best_"))
        );
    }
}
