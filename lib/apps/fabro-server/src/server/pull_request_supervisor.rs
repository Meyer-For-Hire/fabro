//! Background processing for durably accepted pull request creations.
//!
//! `POST /runs/{id}/pull_request` records a `pull_request.creation_requested`
//! event and returns 202; this supervisor finds pending creations (including
//! after a server restart), runs them under a bounded worker pool, and
//! records a durable success or failure result.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use fabro_types::{PullRequestCreation, PullRequestCreationId, RunId};
use tokio::task::{self, JoinHandle, JoinSet};
use tokio::time;
use tracing::{Instrument as _, info_span, warn};

use super::handler::pull_requests::{
    RunPrInputs, load_server_github_credentials, server_github_context,
};
use super::{AppState, pull_request, workflow_event};

const PULL_REQUEST_CREATION_TIMEOUT: Duration = Duration::from_mins(10);
const PULL_REQUEST_CREATION_SCAN_INTERVAL: Duration = Duration::from_secs(30);
const MAX_CONCURRENT_PULL_REQUEST_CREATIONS: usize = 4;
/// Stop retrying a run after this many worker attempts that could not even
/// record a durable failure (store errors). Without a cap, such a run would
/// re-run the whole attempt — including the LLM call — on every scan.
const MAX_WORKER_FAILURES_PER_RUN: u32 = 3;

async fn append_pull_request_creation_failure(
    run_store: &fabro_store::RunDatabase,
    run_id: &RunId,
    creation_id: PullRequestCreationId,
    error: String,
) -> anyhow::Result<()> {
    let event = workflow_event::Event::PullRequestFailed {
        creation_id: Some(creation_id),
        error,
    };
    workflow_event::append_event_if(run_store, run_id, &event, |projection| {
        is_pending_creation(projection, creation_id)
    })
    .await?;
    Ok(())
}

fn is_pending_creation(
    projection: &fabro_store::RunProjection,
    creation_id: PullRequestCreationId,
) -> bool {
    projection
        .pull_request_creation
        .as_ref()
        .is_some_and(|creation| creation.id == creation_id && creation.is_pending())
}

pub(in crate::server) async fn process_pull_request_creation(
    state: Arc<AppState>,
    run_id: RunId,
) -> anyhow::Result<()> {
    let _create_guard = state.pull_request_create_locks.lock(run_id).await;
    let run_store = state.stores.runs.open_run(&run_id).await?;
    let Some(run_state) = state.stores.runs.get_cached_projection(&run_id).await? else {
        return Ok(());
    };
    let Some(creation) = run_state
        .pull_request_creation
        .as_ref()
        .filter(|creation| creation.is_pending())
        .cloned()
    else {
        return Ok(());
    };

    match attempt_pull_request_creation(&state, &run_store, &run_id, &run_state, &creation).await? {
        Ok(()) => Ok(()),
        Err(error) => {
            append_pull_request_creation_failure(&run_store, &run_id, creation.id, error).await
        }
    }
}

/// One end-to-end creation attempt. The inner `Err` is a durable creation
/// failure for the caller to record; the inner `Ok` covers success and
/// shutdown-interrupted attempts (which stay pending). The outer `Err` is an
/// infrastructure failure — nothing was recorded, so the supervisor may retry.
async fn attempt_pull_request_creation(
    state: &AppState,
    run_store: &fabro_store::RunDatabase,
    run_id: &RunId,
    run_state: &fabro_store::RunProjection,
    creation: &PullRequestCreation,
) -> anyhow::Result<Result<(), String>> {
    let inputs = match RunPrInputs::extract(run_state, creation.force) {
        Ok(inputs) => inputs,
        Err(err) => return Ok(Err(err.detail().to_string())),
    };
    let creds = match load_server_github_credentials(state).await {
        Ok(creds) => creds,
        Err(err) => return Ok(Err(err.detail().to_string())),
    };
    let github = match server_github_context(state, &creds) {
        Ok(github) => github,
        Err(err) => return Ok(Err(err.detail().to_string())),
    };
    let catalog = state.catalog();
    let run_store_handle = run_store.clone().into();
    let request = pull_request::OpenPullRequestRequest {
        github,
        origin_url: &inputs.normalized_origin,
        base_branch: inputs.base_branch,
        head_branch: inputs.run_branch,
        expected_head_sha: inputs.final_git_sha,
        goal: inputs.goal,
        diff: inputs.diff,
        model: &creation.model,
        draft: true,
        auto_merge: None,
        run_store: &run_store_handle,
        llm_source: state.llm_source.as_ref(),
        catalog,
        conclusion: Some(inputs.conclusion),
        run_state: Some(run_state),
    };
    let shutdown = state.shutdown_token();
    let result = tokio::select! {
        () = shutdown.cancelled() => return Ok(Ok(())),
        result = time::timeout(PULL_REQUEST_CREATION_TIMEOUT, pull_request::open_pull_request(request)) => result,
    };
    let created_pull_request = match result {
        Ok(Ok(created)) => created,
        Ok(Err(err)) => return Ok(Err(err)),
        Err(_) => {
            return Ok(Err(format!(
                "Pull request creation timed out after {} minutes.",
                PULL_REQUEST_CREATION_TIMEOUT.as_secs() / 60
            )));
        }
    };

    let event = workflow_event::Event::pull_request_created(
        &created_pull_request.link,
        &created_pull_request.base_branch,
        &created_pull_request.head_branch,
        inputs.final_git_sha,
        &created_pull_request.title,
        true,
    );
    workflow_event::append_event_if(run_store, run_id, &event, |projection| {
        projection.pull_request.is_none() && is_pending_creation(projection, creation.id)
    })
    .await?;
    Ok(Ok(()))
}

pub(crate) fn spawn_pull_request_creation_supervisor(state: Arc<AppState>) -> JoinHandle<()> {
    tokio::spawn(
        run_pull_request_creation_supervisor(state)
            .instrument(info_span!("pull_request_creation_supervisor")),
    )
}

async fn run_pull_request_creation_supervisor(state: Arc<AppState>) {
    let shutdown = state.shutdown_token();
    let mut workers = JoinSet::new();
    let mut active: HashMap<task::Id, RunId> = HashMap::new();
    let mut failures: HashMap<RunId, u32> = HashMap::new();
    let mut scan_requested = true;
    let mut scan_interval = time::interval(PULL_REQUEST_CREATION_SCAN_INTERVAL);
    scan_interval.set_missed_tick_behavior(time::MissedTickBehavior::Delay);

    loop {
        if scan_requested {
            match state
                .stores
                .runs
                .pending_pull_request_creation_run_ids()
                .await
            {
                Ok(pending) => {
                    let available =
                        MAX_CONCURRENT_PULL_REQUEST_CREATIONS.saturating_sub(active.len());
                    let ready = pending
                        .into_iter()
                        .filter(|run_id| {
                            !active.values().any(|active_id| active_id == run_id)
                                && failures.get(run_id).copied().unwrap_or(0)
                                    < MAX_WORKER_FAILURES_PER_RUN
                        })
                        .take(available)
                        .collect::<Vec<_>>();
                    for run_id in ready {
                        let handle = workers.spawn(
                            process_pull_request_creation(Arc::clone(&state), run_id)
                                .instrument(info_span!("pull_request_creation", run_id = %run_id)),
                        );
                        active.insert(handle.id(), run_id);
                    }
                }
                Err(err) => {
                    warn!(error = %err, "Failed to scan queued pull request creations");
                }
            }
            scan_requested = false;
        }

        if shutdown.is_cancelled() {
            break;
        }

        if workers.is_empty() {
            tokio::select! {
                () = shutdown.cancelled() => break,
                () = state.pull_request_scheduler_notified() => scan_requested = true,
                _ = scan_interval.tick() => scan_requested = true,
            }
            continue;
        }

        tokio::select! {
            () = shutdown.cancelled() => break,
            () = state.pull_request_scheduler_notified() => scan_requested = true,
            _ = scan_interval.tick() => scan_requested = true,
            joined = workers.join_next_with_id() => {
                match joined {
                    Some(Ok((task_id, result))) => {
                        let run_id = active.remove(&task_id);
                        match (run_id, result) {
                            (Some(run_id), Ok(())) => {
                                failures.remove(&run_id);
                                scan_requested = true;
                            }
                            (Some(run_id), Err(err)) => {
                                // Deliberately no immediate rescan: the run's
                                // creation is still pending, and re-picking it
                                // now would retry the whole attempt in a tight
                                // loop. The next interval tick retries it.
                                *failures.entry(run_id).or_default() += 1;
                                warn!(run_id = %run_id, error = %err, "Pull request creation worker failed");
                            }
                            (None, result) => {
                                warn!(?result, "Pull request creation worker finished without a tracked run id");
                            }
                        }
                    }
                    Some(Err(err)) => {
                        if let Some(run_id) = active.remove(&err.id()) {
                            *failures.entry(run_id).or_default() += 1;
                            warn!(run_id = %run_id, error = %err, "Pull request creation worker stopped unexpectedly");
                        } else {
                            warn!(error = %err, "Pull request creation worker stopped unexpectedly");
                        }
                    }
                    None => {}
                }
            }
        }
    }

    while let Some(joined) = workers.join_next().await {
        if let Err(err) = joined {
            warn!(error = %err, "Pull request creation worker stopped during shutdown");
        }
    }
}
