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
use crate::context::{Context, ParallelBranchPreamble, WorkflowContext, context_diff_public, keys};
use crate::error::Error;
use crate::event::{Emitter, Event, RunNoticeCode, RunNoticeLevel, StageScope};
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

/// Parse the per-branch preamble stash produced by `FidelityLifecycle`.
///
/// Outer `None` means the stash is absent, malformed, or has the wrong branch
/// count — every branch then inherits the fork context (legacy behavior).
/// Inner `None` means that single branch inherits.
fn parse_branch_preambles(
    value: Option<serde_json::Value>,
    branch_count: usize,
) -> Option<Vec<Option<ParallelBranchPreamble>>> {
    let serde_json::Value::Array(entries) = value? else {
        return None;
    };
    if entries.len() != branch_count {
        return None;
    }

    entries
        .into_iter()
        .map(|entry| match entry {
            serde_json::Value::Null => Some(None),
            entry => serde_json::from_value(entry).ok().map(Some),
        })
        .collect()
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

    let branch_preambles = parse_branch_preambles(
        context.get(keys::INTERNAL_PARALLEL_BRANCH_PREAMBLES),
        branches.len(),
    );
    // Clear the stash before snapshotting so branch contexts never carry the
    // outer array — a nested parallel branch target must not misread it as
    // its own. The write-back diff also clears it on the run state.
    context.set(
        keys::INTERNAL_PARALLEL_BRANCH_PREAMBLES,
        serde_json::Value::Null,
    );
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
        if let Some(entry) = branch_preambles
            .as_ref()
            .and_then(|entries| entries.get(branch_index))
            .and_then(Option::as_ref)
        {
            branch_context.set(
                keys::CURRENT_PREAMBLE,
                serde_json::Value::String(entry.preamble.clone()),
            );
            branch_context.set(
                keys::INTERNAL_FIDELITY,
                serde_json::Value::String(entry.fidelity.to_string()),
            );
        }

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
                    emit_branch_completed(
                        &branch_services.run.emitter,
                        &branch_scope,
                        group_id.clone(),
                        parallel_branch_id.clone(),
                        branch_index,
                        millis_u64(branch_start.elapsed()),
                        outcome.status,
                    );
                    Ok::<BranchResult, Error>(BranchResult { result, outcome })
                };

                match std::panic::AssertUnwindSafe(task).catch_unwind().await {
                    Ok(result) => result,
                    Err(payload) => {
                        let result =
                            failed_branch_result(&target_id, super::format_panic_message(&payload));
                        emit_branch_completed(
                            &branch_services.run.emitter,
                            &branch_scope,
                            group_id,
                            parallel_branch_id,
                            branch_index,
                            millis_u64(branch_start.elapsed()),
                            result.outcome.status,
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
            emit_branch_completed(
                &services.run.emitter,
                &dispatch.scope,
                parallel_group_id.clone(),
                dispatch.branch_id,
                dispatch.index,
                0,
                result.outcome.status,
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
    updates.extend(context_diff_public(before, after));
    updates
}

/// Emit `ParallelBranchCompleted` for the branch that `scope` identifies;
/// `scope.node_id` is the branch target by construction
/// ([`StageScope::for_parallel_branch`]).
fn emit_branch_completed(
    emitter: &Emitter,
    scope: &StageScope,
    parallel_group_id: StageId,
    parallel_branch_id: ParallelBranchId,
    index: usize,
    duration_ms: u64,
    status: StageOutcome,
) {
    emitter.emit_scoped(
        &Event::ParallelBranchCompleted {
            parallel_group_id,
            parallel_branch_id,
            branch: scope.node_id.clone(),
            index,
            duration_ms,
            status,
        },
        scope,
    );
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
    use std::sync::{Arc, Mutex};
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

    #[derive(Clone, Debug, PartialEq)]
    struct BranchContextCapture {
        node_id:  String,
        preamble: String,
        fidelity: String,
        stash:    Option<serde_json::Value>,
    }

    struct BranchContextRecordingHandler {
        captures: Arc<Mutex<Vec<BranchContextCapture>>>,
    }

    #[async_trait]
    impl Handler for BranchContextRecordingHandler {
        async fn execute(
            &self,
            node: &Node,
            context: &Context,
            _graph: &Graph,
            _run_dir: &Path,
            _services: &EngineServices,
        ) -> Result<Outcome, Error> {
            self.captures.lock().unwrap().push(BranchContextCapture {
                node_id:  node.id.clone(),
                preamble: context.preamble(),
                fidelity: context.fidelity().to_string(),
                stash:    context.get(keys::INTERNAL_PARALLEL_BRANCH_PREAMBLES),
            });
            Ok(Outcome::success())
        }
    }

    async fn execute_with_branch_stash(
        stash: Option<serde_json::Value>,
        duplicate_target: bool,
    ) -> (Context, Vec<BranchContextCapture>) {
        let captures = Arc::new(Mutex::new(Vec::new()));
        let recorder = BranchContextRecordingHandler {
            captures: Arc::clone(&captures),
        };
        let mut registry = super::super::HandlerRegistry::new(Box::new(recorder));
        registry.register(
            "record",
            Box::new(BranchContextRecordingHandler {
                captures: Arc::clone(&captures),
            }),
        );
        let mut services = EngineServices::test_default();
        services.registry = Arc::new(registry);

        let mut node = Node::new("par");
        node.attrs.insert(
            "shape".to_string(),
            AttrValue::String("component".to_string()),
        );
        let mut branch_a = Node::new("branch_a");
        branch_a
            .attrs
            .insert("type".to_string(), AttrValue::String("record".to_string()));
        let mut branch_b = Node::new("branch_b");
        branch_b
            .attrs
            .insert("type".to_string(), AttrValue::String("record".to_string()));

        let mut graph = Graph::new("test");
        graph.nodes.insert(node.id.clone(), node.clone());
        graph.nodes.insert(branch_a.id.clone(), branch_a);
        graph.nodes.insert(branch_b.id.clone(), branch_b);
        graph.edges.push(Edge::new("par", "branch_a"));
        graph.edges.push(Edge::new(
            "par",
            if duplicate_target {
                "branch_a"
            } else {
                "branch_b"
            },
        ));

        let context = test_context();
        context.set(keys::CURRENT_PREAMBLE, serde_json::json!("fork preamble"));
        context.set(keys::INTERNAL_FIDELITY, serde_json::json!("compact"));
        if let Some(stash) = stash {
            context.set(keys::INTERNAL_PARALLEL_BRANCH_PREAMBLES, stash);
        }

        let run_dir = tempfile::tempdir().unwrap();
        ParallelHandler
            .execute(&node, &context, &graph, run_dir.path(), &services)
            .await
            .unwrap();

        let captures = captures.lock().unwrap().clone();
        (context, captures)
    }

    #[tokio::test]
    async fn parallel_handler_applies_indexed_branch_preambles_and_clears_stash() {
        let stash = serde_json::json!([
            {"fidelity": "truncate", "preamble": "branch zero"},
            {"fidelity": "summary:high", "preamble": "branch one"}
        ]);

        let (context, mut captures) = execute_with_branch_stash(Some(stash), false).await;
        captures.sort_by(|left, right| left.node_id.cmp(&right.node_id));

        assert_eq!(captures.len(), 2);
        assert_eq!(captures[0].node_id, "branch_a");
        assert_eq!(captures[0].preamble, "branch zero");
        assert_eq!(captures[0].fidelity, "truncate");
        assert_eq!(captures[0].stash, Some(serde_json::Value::Null));
        assert_eq!(captures[1].node_id, "branch_b");
        assert_eq!(captures[1].preamble, "branch one");
        assert_eq!(captures[1].fidelity, "summary:high");
        assert_eq!(captures[1].stash, Some(serde_json::Value::Null));
        assert_eq!(
            context.get(keys::INTERNAL_PARALLEL_BRANCH_PREAMBLES),
            Some(serde_json::Value::Null)
        );
    }

    #[tokio::test]
    async fn parallel_handler_uses_edge_index_for_duplicate_targets() {
        let stash = serde_json::json!([
            {"fidelity": "truncate", "preamble": "first edge"},
            {"fidelity": "summary:low", "preamble": "second edge"}
        ]);

        let (_context, captures) = execute_with_branch_stash(Some(stash), true).await;
        let observed = captures
            .iter()
            .map(|capture| (capture.preamble.as_str(), capture.fidelity.as_str()))
            .collect::<std::collections::HashSet<_>>();

        assert_eq!(observed.len(), 2);
        assert!(observed.contains(&("first edge", "truncate")));
        assert!(observed.contains(&("second edge", "summary:low")));
        assert!(
            captures
                .iter()
                .all(|capture| capture.stash == Some(serde_json::Value::Null))
        );
    }

    #[tokio::test]
    async fn parallel_handler_legacy_stashes_inherit_fork_context() {
        for stash in [
            None,
            Some(serde_json::Value::Null),
            Some(serde_json::json!({
                "fidelity": "truncate",
                "preamble": "not an array"
            })),
            Some(serde_json::json!([
                {"fidelity": "truncate", "preamble": "wrong length"}
            ])),
            Some(serde_json::json!([
                {"fidelity": "truncate"},
                null
            ])),
            Some(serde_json::json!([
                {"fidelity": "not-a-fidelity", "preamble": "malformed fidelity"},
                null
            ])),
        ] {
            let (context, captures) = execute_with_branch_stash(stash, false).await;

            assert_eq!(captures.len(), 2);
            assert!(captures.iter().all(|capture| {
                capture.preamble == "fork preamble"
                    && capture.fidelity == "compact"
                    && capture.stash == Some(serde_json::Value::Null)
            }));
            assert_eq!(
                context.get(keys::INTERNAL_PARALLEL_BRANCH_PREAMBLES),
                Some(serde_json::Value::Null)
            );
        }
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
