use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use fabro_graphviz::graph::{AttrValue, Graph, Node};
use fabro_hooks::{HookContext, HookEvent};
use fabro_types::{ParallelBranchId, ParallelBranchResult, StageId, StageOutcome};
use futures::FutureExt;
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;

use super::{EngineServices, Handler};
use crate::context::{Context, WorkflowContext, context_diff, keys};
use crate::error::Error;
use crate::event::{Event, RunNoticeCode, RunNoticeLevel, StageScope};
use crate::hook_context::set_hook_node;
use crate::outcome::{FailureCategory, FailureDetail, Outcome, OutcomeExt};
use crate::{artifact, millis_u64};

/// Fans out execution to multiple branches concurrently.
/// Each branch gets an isolated context fork and shares the run sandbox.
pub struct ParallelHandler;

struct BranchResult {
    result:  ParallelBranchResult,
    outcome: Outcome,
}

struct BranchDispatch {
    index:     usize,
    target_id: String,
    branch_id: ParallelBranchId,
    scope:     StageScope,
    handle:    JoinHandle<Result<BranchResult, Error>>,
}

#[async_trait]
impl Handler for ParallelHandler {
    async fn simulate(
        &self,
        node: &Node,
        context: &Context,
        graph: &Graph,
        run_dir: &Path,
        services: &EngineServices,
    ) -> Result<Outcome, Error> {
        run_branches(node, context, graph, run_dir, services, true).await
    }

    async fn execute(
        &self,
        node: &Node,
        context: &Context,
        graph: &Graph,
        run_dir: &Path,
        services: &EngineServices,
    ) -> Result<Outcome, Error> {
        run_branches(node, context, graph, run_dir, services, false).await
    }
}

async fn run_branches(
    node: &Node,
    context: &Context,
    graph: &Graph,
    run_dir: &Path,
    services: &EngineServices,
    simulated: bool,
) -> Result<Outcome, Error> {
    let parallel_start = Instant::now();
    let branches = graph.outgoing_edges(&node.id);

    let parallel_stage_scope = StageScope::for_handler(context, &node.id);
    let parallel_group_id = StageId::new(node.id.clone(), parallel_stage_scope.visit);
    services.run.emitter.emit_scoped(
        &Event::ParallelStarted {
            node_id:      node.id.clone(),
            visit:        parallel_stage_scope.visit,
            branch_count: branches.len(),
        },
        &parallel_stage_scope,
    );
    emit_parallel_hook(services, context, graph, node, HookEvent::ParallelStart).await?;

    let max_parallel = node
        .attrs
        .get("max_parallel")
        .and_then(AttrValue::as_i64)
        .unwrap_or(4);
    let max_parallel = usize::try_from(max_parallel).unwrap_or(4).max(1);
    let semaphore = Arc::new(Semaphore::new(max_parallel));
    let shared_graph = Arc::new(graph.clone());
    let parent_snapshot = Arc::new(context.snapshot());

    let mut dispatches = Vec::with_capacity(branches.len());
    for (branch_index, edge) in branches.iter().enumerate() {
        let target_id = edge.to.clone();
        let parallel_branch_id = ParallelBranchId::new(
            parallel_group_id.clone(),
            u32::try_from(branch_index).unwrap_or(u32::MAX),
        );
        let branch_context = Context::from_values(parent_snapshot.as_ref().clone());
        branch_context.set(
            keys::INTERNAL_PARALLEL_GROUP_ID,
            serde_json::Value::String(parallel_group_id.to_string()),
        );
        branch_context.set(
            keys::INTERNAL_PARALLEL_BRANCH_ID,
            serde_json::Value::String(parallel_branch_id.to_string()),
        );

        let mut branch_services = services.clone();
        branch_services.dry_run = simulated || services.dry_run;
        let parent_snapshot = Arc::clone(&parent_snapshot);
        let graph = Arc::clone(&shared_graph);
        let run_dir = run_dir.to_path_buf();
        let semaphore = Arc::clone(&semaphore);
        let group_id = parallel_group_id.clone();
        let branch_scope = StageScope::for_parallel_branch(
            target_id.clone(),
            1,
            group_id.clone(),
            parallel_branch_id.clone(),
        );

        dispatches.push(BranchDispatch {
            index:     branch_index,
            target_id: target_id.clone(),
            branch_id: parallel_branch_id.clone(),
            scope:     branch_scope.clone(),
            handle:    tokio::spawn(async move {
                let branch_start = Instant::now();
                let task = async {
                    let permit = semaphore.acquire();
                    tokio::pin!(permit);
                    let cancel_token = branch_services.run.cancel_token();
                    let _permit = tokio::select! {
                        biased;
                        () = cancel_token.cancelled() => {
                            return Err(Error::Cancelled);
                        }
                        permit = &mut permit => permit
                            .map_err(|err| Error::handler_with_source("semaphore error", err))?,
                    };
                    branch_services.run.emitter.emit_scoped(
                        &Event::ParallelBranchStarted {
                            parallel_group_id:  group_id.clone(),
                            parallel_branch_id: parallel_branch_id.clone(),
                            branch:             target_id.clone(),
                            index:              branch_index,
                        },
                        &branch_scope,
                    );

                    let outcome = match graph.nodes.get(&target_id) {
                        Some(target_node) => {
                            let handler = branch_services.registry.resolve(target_node);
                            match super::dispatch_handler(
                                handler,
                                target_node,
                                &branch_context,
                                &graph,
                                &run_dir,
                                &branch_services,
                            )
                            .await
                            {
                                Ok(outcome) => outcome,
                                Err(Error::Cancelled) => return Err(Error::Cancelled),
                                Err(err) => err.to_fail_outcome(),
                            }
                        }
                        None => Outcome::fail_classify(format!(
                            "branch target node not found: {target_id}"
                        )),
                    };

                    let context_updates = branch_context_updates(
                        &parent_snapshot,
                        branch_context.snapshot(),
                        &outcome.context_updates,
                    );
                    let result = ParallelBranchResult {
                        id: target_id.clone(),
                        status: outcome.status,
                        context_updates,
                    };
                    branch_services.run.emitter.emit_scoped(
                        &Event::ParallelBranchCompleted {
                            parallel_group_id:  group_id.clone(),
                            parallel_branch_id: parallel_branch_id.clone(),
                            branch:             target_id.clone(),
                            index:              branch_index,
                            duration_ms:        millis_u64(branch_start.elapsed()),
                            status:             result.status,
                        },
                        &branch_scope,
                    );
                    Ok::<BranchResult, Error>(BranchResult { result, outcome })
                };

                match std::panic::AssertUnwindSafe(task).catch_unwind().await {
                    Ok(result) => result,
                    Err(payload) => {
                        let result =
                            failed_branch_result(&target_id, super::format_panic_message(&payload));
                        branch_services.run.emitter.emit_scoped(
                            &Event::ParallelBranchCompleted {
                                parallel_group_id: group_id,
                                parallel_branch_id,
                                branch: target_id,
                                index: branch_index,
                                duration_ms: millis_u64(branch_start.elapsed()),
                                status: result.result.status,
                            },
                            &branch_scope,
                        );
                        Ok(result)
                    }
                }
            }),
        });
    }

    // Awaiting in dispatch order keeps `results` aligned with the node's
    // outgoing-edge order regardless of branch completion order.
    let mut results = Vec::with_capacity(dispatches.len());
    let mut cancelled = false;
    for dispatch in dispatches {
        let (result, emit_completion) = match dispatch.handle.await {
            Ok(Ok(result)) => (result, false),
            Ok(Err(Error::Cancelled)) => {
                cancelled = true;
                (
                    failed_branch_result(&dispatch.target_id, "branch cancelled"),
                    true,
                )
            }
            Ok(Err(err)) => (
                failed_branch_result(&dispatch.target_id, err.to_string()),
                true,
            ),
            Err(join_err) => (
                failed_branch_result(&dispatch.target_id, format!("task join error: {join_err}")),
                true,
            ),
        };
        if emit_completion {
            services.run.emitter.emit_scoped(
                &Event::ParallelBranchCompleted {
                    parallel_group_id:  parallel_group_id.clone(),
                    parallel_branch_id: dispatch.branch_id,
                    branch:             dispatch.target_id,
                    index:              dispatch.index,
                    duration_ms:        0,
                    status:             result.result.status,
                },
                &dispatch.scope,
            );
        }
        if result.outcome.failure_category() == Some(FailureCategory::Canceled) {
            cancelled = true;
        }
        results.push(result);
    }
    if cancelled {
        return Err(Error::Cancelled);
    }

    let success_count = results
        .iter()
        .filter(|branch| branch.outcome.status == StageOutcome::Succeeded)
        .count();
    let failure_count = results
        .iter()
        .filter(|branch| branch.outcome.status.is_failure())
        .count();
    let total = results.len();
    let status = aggregate_status(&results);
    let is_failure = status.is_failure();
    let jump_to_node = if is_failure {
        None
    } else {
        find_join_node(&results, graph)
    };

    let mut typed_results = results
        .into_iter()
        .map(|branch| branch.result)
        .collect::<Vec<_>>();
    // Offload large leaves before the results reach the event log and
    // projection: the artifact lifecycle's offload pass runs only after the
    // handler returns, too late for the `parallel.completed` payload.
    if let Err(err) =
        artifact::offload_parallel_branch_updates(&mut typed_results, &services.run.run_store).await
    {
        services.run.emitter.notice(
            RunNoticeLevel::Warn,
            RunNoticeCode::ArtifactOffloadFailed,
            format!("[node: {}] parallel result offload failed: {err}", node.id),
        );
    }
    let results_value = serde_json::to_value(&typed_results)
        .map_err(|err| Error::handler_with_source("parallel result serialization failed", err))?;
    let context_updates = HashMap::from([
        (keys::PARALLEL_RESULTS.to_string(), results_value),
        (
            keys::PARALLEL_BRANCH_COUNT.to_string(),
            serde_json::json!(total),
        ),
    ]);

    services.run.emitter.emit_scoped(
        &Event::ParallelCompleted {
            node_id: node.id.clone(),
            visit: parallel_stage_scope.visit,
            duration_ms: millis_u64(parallel_start.elapsed()),
            success_count,
            failure_count,
            results: typed_results,
        },
        &parallel_stage_scope,
    );
    emit_parallel_hook(services, context, graph, node, HookEvent::ParallelComplete).await?;

    let prefix = if simulated { "[Simulated] " } else { "" };
    let mut outcome = Outcome {
        status,
        notes: Some(format!(
            "{prefix}Parallel node dispatched {total} branches ({success_count} succeeded, {failure_count} failed)"
        )),
        failure: is_failure.then(|| {
            FailureDetail::new(
                "All parallel branches failed",
                FailureCategory::Deterministic,
            )
        }),
        jump_to_node,
        context_updates,
        ..Outcome::success()
    };
    if is_failure {
        outcome.suggested_next_ids.clear();
    }
    Ok(outcome)
}

async fn emit_parallel_hook(
    services: &EngineServices,
    context: &Context,
    graph: &Graph,
    node: &Node,
    hook_event: HookEvent,
) -> Result<(), Error> {
    let run_id = context.parsed_run_id()?;
    let mut hook_context = HookContext::new(hook_event, run_id, graph.name.clone());
    set_hook_node(&mut hook_context, node);
    let _ = services.run.run_hooks(&hook_context).await;
    Ok(())
}

fn branch_context_updates(
    before: &HashMap<String, serde_json::Value>,
    after: HashMap<String, serde_json::Value>,
    outcome_updates: &HashMap<String, serde_json::Value>,
) -> BTreeMap<String, serde_json::Value> {
    let mut updates = outcome_updates
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect::<BTreeMap<_, _>>();
    updates.extend(
        context_diff(before, after)
            .into_iter()
            .filter(|(key, _)| !keys::is_engine_internal_key(key)),
    );
    updates
}

fn failed_branch_result(id: &str, reason: impl Into<String>) -> BranchResult {
    let outcome = Outcome::fail_classify(reason);
    BranchResult {
        result: ParallelBranchResult {
            id:              id.to_string(),
            status:          outcome.status,
            context_updates: BTreeMap::new(),
        },
        outcome,
    }
}

fn aggregate_status(results: &[BranchResult]) -> StageOutcome {
    if results.is_empty() {
        StageOutcome::PartiallySucceeded
    } else if results
        .iter()
        .all(|result| result.outcome.status == StageOutcome::Succeeded)
    {
        StageOutcome::Succeeded
    } else if results
        .iter()
        .all(|result| result.outcome.status.is_failure())
    {
        StageOutcome::Failed {
            retry_requested: false,
        }
    } else {
        StageOutcome::PartiallySucceeded
    }
}

/// Find the convergence node by finding a common direct target of every branch.
fn find_join_node(results: &[BranchResult], graph: &Graph) -> Option<String> {
    let first_result = results.first()?;
    let first_targets = graph
        .outgoing_edges(&first_result.result.id)
        .into_iter()
        .map(|edge| edge.to.clone())
        .collect::<HashSet<_>>();
    let mut common = first_targets
        .into_iter()
        .filter(|target| {
            results.iter().skip(1).all(|result| {
                graph
                    .outgoing_edges(&result.result.id)
                    .into_iter()
                    .any(|edge| &edge.to == target)
            })
        })
        .collect::<Vec<_>>();
    common.sort();
    common.into_iter().next()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use fabro_graphviz::graph::{AttrValue, Edge};
    use fabro_store::{Database, StageId};
    use fabro_types::{fixtures, test_support};
    use object_store::memory::InMemory;

    use super::*;

    fn make_services() -> EngineServices {
        EngineServices::test_default()
    }

    fn test_store() -> Arc<Database> {
        Arc::new(Database::new(
            Arc::new(InMemory::new()),
            "",
            Duration::from_millis(1),
            None,
        ))
    }

    async fn seed_created(run_store: &fabro_store::RunDatabase) {
        crate::event::append_event(
            run_store,
            &fixtures::RUN_1,
            &crate::event::Event::RunCreated {
                run_id:           fixtures::RUN_1,
                title:            None,
                settings:         serde_json::to_value(fabro_types::WorkflowSettings::default())
                    .unwrap(),
                graph:            serde_json::to_value(fabro_types::Graph::new("test")).unwrap(),
                workflow_source:  None,
                workflow_config:  None,
                labels:           BTreeMap::default(),
                run_dir:          "/tmp".to_string(),
                source_directory: None,
                workflow_slug:    None,
                automation:       None,
                db_prefix:        None,
                provenance:       test_support::test_run_provenance(),
                manifest_blob:    None,
                git:              None,
                fork_source_ref:  None,
                retried_from:     None,
                parent_id:        None,
                web_url:          None,
            },
        )
        .await
        .unwrap();
    }

    fn test_context() -> Context {
        let context = Context::new();
        context.set(
            keys::INTERNAL_RUN_ID,
            serde_json::json!(fixtures::RUN_1.to_string()),
        );
        context
    }

    fn parallel_graph() -> (Node, Graph) {
        let mut node = Node::new("par");
        node.attrs.insert(
            "shape".to_string(),
            AttrValue::String("component".to_string()),
        );
        let mut graph = Graph::new("test");
        graph.nodes.insert("par".to_string(), node.clone());
        graph
            .nodes
            .insert("branch_a".to_string(), Node::new("branch_a"));
        graph
            .nodes
            .insert("branch_b".to_string(), Node::new("branch_b"));
        graph.edges.push(Edge::new("par", "branch_a"));
        graph.edges.push(Edge::new("par", "branch_b"));
        (node, graph)
    }

    #[tokio::test]
    async fn parallel_handler_no_branches() {
        let outcome = ParallelHandler
            .execute(
                &Node::new("par"),
                &test_context(),
                &Graph::new("test"),
                Path::new("/tmp/test"),
                &make_services(),
            )
            .await
            .unwrap();
        assert_eq!(outcome.status, StageOutcome::PartiallySucceeded);
        assert_eq!(
            outcome.context_updates[keys::PARALLEL_RESULTS],
            serde_json::json!([])
        );
        assert_eq!(
            outcome.context_updates[keys::PARALLEL_BRANCH_COUNT],
            serde_json::json!(0)
        );
    }

    #[tokio::test]
    async fn parallel_handler_returns_typed_ordered_results() {
        let store = test_store();
        let run_store = store.create_run(&fixtures::RUN_1).await.unwrap();
        seed_created(&run_store).await;
        let mut services = make_services();
        services.run = services
            .run
            .with_emitter(Arc::new(crate::event::Emitter::new(fixtures::RUN_1)))
            .with_run_store(run_store.clone().into());
        let logger = crate::event::StoreProgressLogger::new(run_store.clone());
        logger.register(services.run.emitter.as_ref());
        let (node, graph) = parallel_graph();
        let context = test_context();

        let outcome = ParallelHandler
            .execute(&node, &context, &graph, Path::new("/tmp/test"), &services)
            .await
            .unwrap();
        logger.flush().await;

        assert_eq!(outcome.status, StageOutcome::Succeeded);
        let results: Vec<ParallelBranchResult> =
            serde_json::from_value(outcome.context_updates[keys::PARALLEL_RESULTS].clone())
                .unwrap();
        assert_eq!(
            results
                .iter()
                .map(|result| result.id.as_str())
                .collect::<Vec<_>>(),
            ["branch_a", "branch_b"]
        );
        assert!(
            results
                .iter()
                .all(|result| result.status == StageOutcome::Succeeded)
        );
        let state = run_store.state().await.unwrap();
        assert_eq!(
            state
                .stage(&StageId::new("par", 1))
                .unwrap()
                .parallel_results
                .as_ref()
                .unwrap()
                .len(),
            2
        );
    }

    #[tokio::test]
    async fn parallel_handler_simulate_returns_results_as_outcome_updates() {
        let (node, graph) = parallel_graph();
        let context = test_context();
        let outcome = ParallelHandler
            .simulate(
                &node,
                &context,
                &graph,
                Path::new("/tmp/test"),
                &make_services(),
            )
            .await
            .unwrap();

        assert_eq!(outcome.status, StageOutcome::Succeeded);
        assert!(outcome.notes.as_deref().unwrap().contains("[Simulated]"));
        assert_eq!(
            outcome.context_updates[keys::PARALLEL_BRANCH_COUNT],
            serde_json::json!(2)
        );
    }

    #[test]
    fn aggregate_status_follows_parallel_truth_table() {
        let success = |index: usize| BranchResult {
            result:  ParallelBranchResult {
                id:              format!("branch_{index}"),
                status:          StageOutcome::Succeeded,
                context_updates: BTreeMap::new(),
            },
            outcome: Outcome::success(),
        };
        let failure = |index: usize| failed_branch_result(&format!("branch_{index}"), "failed");
        let partial = |index: usize| BranchResult {
            result:  ParallelBranchResult {
                id:              format!("branch_{index}"),
                status:          StageOutcome::PartiallySucceeded,
                context_updates: BTreeMap::new(),
            },
            outcome: Outcome {
                status: StageOutcome::PartiallySucceeded,
                ..Outcome::success()
            },
        };

        assert_eq!(aggregate_status(&[]), StageOutcome::PartiallySucceeded);
        assert_eq!(
            aggregate_status(&[success(0), success(1)]),
            StageOutcome::Succeeded
        );
        assert!(aggregate_status(&[failure(0), failure(1)]).is_failure());
        assert_eq!(
            aggregate_status(&[success(0), failure(1)]),
            StageOutcome::PartiallySucceeded
        );
        assert_eq!(
            aggregate_status(&[success(0), partial(1)]),
            StageOutcome::PartiallySucceeded
        );
        assert_eq!(
            aggregate_status(&[failure(0), partial(1)]),
            StageOutcome::PartiallySucceeded
        );
    }

    #[test]
    fn branch_context_updates_include_failed_outcome_updates_without_internal_keys() {
        let before = HashMap::from([("shared".to_string(), serde_json::json!("parent"))]);
        let after = HashMap::from([
            ("shared".to_string(), serde_json::json!("branch")),
            (
                keys::INTERNAL_WORK_DIR.to_string(),
                serde_json::json!("/workspace"),
            ),
        ]);
        let outcome = HashMap::from([(
            keys::COMMAND_OUTPUT.to_string(),
            serde_json::json!({"stdout": "failure output"}),
        )]);

        assert_eq!(
            branch_context_updates(&before, after, &outcome),
            BTreeMap::from([
                (
                    keys::COMMAND_OUTPUT.to_string(),
                    serde_json::json!({"stdout": "failure output"})
                ),
                ("shared".to_string(), serde_json::json!("branch")),
            ])
        );
    }
}
