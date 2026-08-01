use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use axum::http::{HeaderValue, header};
use fabro_store::ListRunsQuery;
use tokio::task::{JoinHandle, JoinSet};
use tokio::time;
use tracing::Instrument as _;

use super::super::{
    ApiError, AppState, CloseRunPullRequestResponse, CreateRunPullRequestRequest, IntoResponse,
    Json, LinkRunPullRequestRequest, MergeRunPullRequestRequest, MergeRunPullRequestResponse,
    PullRequestLink, RequireRunScoped, Response, Router, RunId, State, StatusCode, get,
    lock_pull_request_create, post, pull_request, warn, workflow_event,
};

pub(super) fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/runs/{id}/pull_request",
            get(get_run_pull_request)
                .post(create_run_pull_request)
                .put(link_run_pull_request)
                .delete(unlink_run_pull_request),
        )
        .route(
            "/runs/{id}/pull_request/merge",
            post(merge_run_pull_request),
        )
        .route(
            "/runs/{id}/pull_request/creation",
            get(get_run_pull_request_creation),
        )
        .route(
            "/runs/{id}/pull_request/close",
            post(close_run_pull_request),
        )
}

const PULL_REQUEST_CREATION_TIMEOUT: Duration = Duration::from_mins(10);
const PULL_REQUEST_CREATION_SCAN_INTERVAL: Duration = Duration::from_secs(5);
const MAX_CONCURRENT_PULL_REQUEST_CREATIONS: usize = 4;

#[expect(
    clippy::disallowed_types,
    reason = "Pull-request API validates public github.com URLs; these raw URLs are not credential-bearing log output."
)]
fn parse_github_owner_repo_from_url(url: &str, kind: &str) -> Result<(String, String), ApiError> {
    let parsed = fabro_http::Url::parse(url)
        .map_err(|err| ApiError::bad_request(format!("Invalid {kind}: {err}")))?;
    match parsed.host_str() {
        Some("github.com") => {}
        Some(host) => {
            return Err(ApiError::with_code(
                StatusCode::BAD_REQUEST,
                format!("Pull request operations support github.com only (got {host})."),
                "unsupported_host",
            ));
        }
        None => {
            return Err(ApiError::bad_request(format!(
                "Invalid {kind}: missing host"
            )));
        }
    }

    fabro_github::parse_github_owner_repo(url).map_err(|err| ApiError::bad_request(err.to_string()))
}

fn pull_request_record_from_link_request(
    body: &LinkRunPullRequestRequest,
) -> Result<PullRequestLink, ApiError> {
    PullRequestLink::from_github_url(body.html_url.trim()).map_err(|err| {
        let code = if err.contains("GitHub pull request URL") {
            "unsupported_pull_request_provider"
        } else {
            "invalid_pull_request_url"
        };
        ApiError::with_code(StatusCode::BAD_REQUEST, err, code)
    })
}

async fn load_server_github_credentials(
    state: &AppState,
) -> Result<fabro_github::GitHubCredentials, ApiError> {
    let settings = state.server_settings();
    match state
        .github_credentials(&settings.server.integrations.github)
        .await
    {
        Ok(Some(creds)) => Ok(creds),
        Ok(None) => {
            warn!("GitHub integration unavailable on server: credentials not configured");
            Err(ApiError::with_code(
                StatusCode::SERVICE_UNAVAILABLE,
                "GitHub integration unavailable on server.",
                "integration_unavailable",
            ))
        }
        Err(err) => {
            warn!(error = %err, "GitHub integration unavailable on server");
            Err(ApiError::with_code(
                StatusCode::SERVICE_UNAVAILABLE,
                "GitHub integration unavailable on server.",
                "integration_unavailable",
            ))
        }
    }
}

fn server_github_context<'a>(
    state: &'a AppState,
    creds: &'a fabro_github::GitHubCredentials,
) -> Result<fabro_github::GitHubContext<'a>, ApiError> {
    let http_client = state.http_client().map_err(|err| {
        ApiError::with_code(
            StatusCode::SERVICE_UNAVAILABLE,
            format!("GitHub integration unavailable on server: {err}"),
            "integration_unavailable",
        )
    })?;
    Ok(fabro_github::GitHubContext::with_http_client(
        creds,
        state.github_api_base_url.as_str(),
        http_client,
    ))
}

fn github_pull_request_not_found_error(number: u64) -> ApiError {
    ApiError::with_code(
        StatusCode::BAD_GATEWAY,
        format!("Pull request #{number} was deleted on GitHub."),
        "github_not_found",
    )
}

struct PullRequestGithubContext {
    record: PullRequestLink,
    owner:  String,
    repo:   String,
    number: u64,
    creds:  fabro_github::GitHubCredentials,
}

async fn load_pull_request_record(
    state: &Arc<AppState>,
    id: &RunId,
) -> Result<PullRequestLink, ApiError> {
    let cached = state.cached_run(id).await?;
    cached.projection.pull_request.clone().ok_or_else(|| {
        ApiError::with_code(
            StatusCode::NOT_FOUND,
            format!("No pull request found in store. Create one first with: fabro pr create {id}"),
            "no_stored_record",
        )
    })
}

fn github_coordinates_for_record(record: &PullRequestLink) -> (String, String, u64) {
    (record.owner.clone(), record.repo.clone(), record.number)
}

async fn load_pull_request_github_context(
    state: &Arc<AppState>,
    id: &RunId,
) -> Result<PullRequestGithubContext, ApiError> {
    let record = load_pull_request_record(state, id).await?;
    let (owner, repo, number) = github_coordinates_for_record(&record);
    let creds = load_server_github_credentials(state.as_ref()).await?;
    Ok(PullRequestGithubContext {
        record,
        owner,
        repo,
        number,
        creds,
    })
}

struct RunPrInputs<'a> {
    goal:              &'a str,
    base_branch:       &'a str,
    run_branch:        &'a str,
    final_git_sha:     &'a str,
    diff:              &'a str,
    conclusion:        &'a fabro_types::Conclusion,
    normalized_origin: String,
}

impl<'a> RunPrInputs<'a> {
    fn extract(run_state: &'a fabro_store::RunProjection, force: bool) -> Result<Self, ApiError> {
        if let Some(record) = run_state.pull_request.as_ref() {
            return Err(ApiError::with_code(
                StatusCode::CONFLICT,
                format!("Pull request already exists at {}", record.html_url()),
                "pull_request_exists",
            ));
        }
        let run_spec = &run_state.spec;
        let origin_url = run_spec.repo_origin_url().ok_or_else(|| {
            ApiError::with_code(
                StatusCode::BAD_REQUEST,
                "Run has no repo origin URL — pull request creation requires git metadata.",
                "missing_repo_origin",
            )
        })?;
        let base_branch = run_spec.base_branch().ok_or_else(|| {
            ApiError::with_code(
                StatusCode::BAD_REQUEST,
                "Run has no base branch — pull request creation requires git metadata.",
                "missing_base_branch",
            )
        })?;
        let run_branch = run_state
            .start
            .as_ref()
            .and_then(|start| start.run_branch.as_deref())
            .ok_or_else(|| {
                ApiError::with_code(
                    StatusCode::BAD_REQUEST,
                    "Run has no run_branch — was it run with git push enabled?",
                    "missing_run_branch",
                )
            })?;
        let diff = run_state
            .conclusion
            .as_ref()
            .and_then(|conclusion| conclusion.diff.patch.as_deref())
            .filter(|d| !d.trim().is_empty())
            .ok_or_else(|| {
                ApiError::with_code(
                    StatusCode::BAD_REQUEST,
                    "Stored diff is empty — nothing to create a PR for",
                    "empty_diff",
                )
            })?;
        let conclusion = run_state.conclusion.as_ref().ok_or_else(|| {
            ApiError::with_code(
                StatusCode::BAD_REQUEST,
                "Run is not finished yet.",
                "run_not_finished",
            )
        })?;
        let final_git_sha = conclusion
            .final_git_commit_sha
            .as_deref()
            .filter(|sha| !sha.trim().is_empty())
            .ok_or_else(|| {
                ApiError::with_code(
                    StatusCode::BAD_REQUEST,
                    "Run has no final git commit SHA — the remote branch cannot be verified.",
                    "missing_final_git_commit",
                )
            })?;
        if !force && !conclusion.status.is_successful() {
            return Err(ApiError::with_code(
                StatusCode::BAD_REQUEST,
                format!(
                    "Run status is '{}', expected succeeded or partially_succeeded",
                    conclusion.status
                ),
                "run_not_successful",
            ));
        }
        let normalized_origin = fabro_github::normalize_repo_origin_url(origin_url);
        parse_github_owner_repo_from_url(&normalized_origin, "repo origin URL")?;
        Ok(Self {
            goal: run_spec.graph.goal(),
            base_branch,
            run_branch,
            final_git_sha,
            diff,
            conclusion,
            normalized_origin,
        })
    }
}

fn unavailable_pull_request_response(
    record: PullRequestLink,
    reason: fabro_types::PullRequestDetailsUnavailableReason,
) -> fabro_types::PullRequestResponse {
    fabro_types::PullRequestResponse {
        data: fabro_types::PullRequest {
            link:    record,
            details: None,
        },
        meta: fabro_types::PullRequestMeta {
            details_status:             fabro_types::PullRequestDetailsStatus::Unavailable,
            details_unavailable_reason: Some(reason),
        },
    }
}

fn available_pull_request_response(
    record: PullRequestLink,
    details: fabro_types::PullRequestDetails,
) -> fabro_types::PullRequestResponse {
    fabro_types::PullRequestResponse {
        data: fabro_types::PullRequest {
            link:    record,
            details: Some(details),
        },
        meta: fabro_types::PullRequestMeta {
            details_status:             fabro_types::PullRequestDetailsStatus::Available,
            details_unavailable_reason: None,
        },
    }
}

async fn create_run_pull_request(
    RequireRunScoped(id): RequireRunScoped,
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateRunPullRequestRequest>,
) -> Response {
    let _create_guard = lock_pull_request_create(&state.pull_request_create_locks, &id).await;
    let Ok(run_store) = state.stores.runs.open_run(&id).await else {
        return ApiError::not_found("Run not found.").into_response();
    };
    let cached = match state.cached_run(&id).await {
        Ok(cached) => cached,
        Err(err) => return err.into_response(),
    };
    let run_state = cached.projection.as_ref();
    if let Some(creation) = run_state
        .pull_request_creation
        .as_ref()
        .filter(|creation| creation.is_pending())
    {
        return accepted_pull_request_creation_response(&id, creation.clone());
    }
    if let Err(err) = RunPrInputs::extract(run_state, body.force) {
        return err.into_response();
    }
    let creds = match load_server_github_credentials(state.as_ref()).await {
        Ok(creds) => creds,
        Err(err) => return err.into_response(),
    };
    if let Err(err) = server_github_context(state.as_ref(), &creds) {
        return err.into_response();
    }
    let model = if let Some(model) = body.model {
        model
    } else {
        let catalog = state.catalog();
        let configured = state.ready_llm_provider_ids().await;
        catalog
            .default_for_configured_ids(&configured)
            .id
            .to_string()
    };
    let creation_id = fabro_types::PullRequestCreationId::new();
    let event = workflow_event::Event::PullRequestCreationRequested {
        creation_id,
        model,
        force: body.force,
    };
    let appended = match workflow_event::append_event_if(&run_store, &id, &event, |projection| {
        projection.pull_request.is_none()
            && !projection
                .pull_request_creation
                .as_ref()
                .is_some_and(fabro_types::PullRequestCreation::is_pending)
    })
    .await
    {
        Ok(appended) => appended,
        Err(err) => {
            return ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
                .into_response();
        }
    };

    let cached = match state.cached_run(&id).await {
        Ok(cached) => cached,
        Err(err) => return err.into_response(),
    };
    if !appended {
        if let Some(creation) = cached
            .projection
            .pull_request_creation
            .as_ref()
            .filter(|creation| creation.is_pending())
        {
            return accepted_pull_request_creation_response(&id, creation.clone());
        }
        if let Some(record) = cached.projection.pull_request.as_ref() {
            return ApiError::with_code(
                StatusCode::CONFLICT,
                format!("Pull request already exists at {}", record.html_url()),
                "pull_request_exists",
            )
            .into_response();
        }
        return ApiError::new(
            StatusCode::CONFLICT,
            "Pull request creation state changed. Retry the request.",
        )
        .into_response();
    }

    let Some(creation) = cached.projection.pull_request_creation.clone() else {
        return ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Pull request creation was accepted but its status is unavailable.",
        )
        .into_response();
    };
    state.notify_pull_request_scheduler();
    accepted_pull_request_creation_response(&id, creation)
}

fn accepted_pull_request_creation_response(
    run_id: &RunId,
    creation: fabro_types::PullRequestCreation,
) -> Response {
    let mut response = (StatusCode::ACCEPTED, Json(creation)).into_response();
    let location = format!("/api/v1/runs/{run_id}/pull_request/creation");
    if let Ok(location) = HeaderValue::from_str(&location) {
        response.headers_mut().insert(header::LOCATION, location);
    }
    response
        .headers_mut()
        .insert(header::RETRY_AFTER, HeaderValue::from_static("1"));
    response
}

async fn get_run_pull_request_creation(
    RequireRunScoped(id): RequireRunScoped,
    State(state): State<Arc<AppState>>,
) -> Response {
    let cached = match state.cached_run(&id).await {
        Ok(cached) => cached,
        Err(err) => return err.into_response(),
    };
    match cached.projection.pull_request_creation.clone() {
        Some(creation) => Json(creation).into_response(),
        None => ApiError::with_code(
            StatusCode::NOT_FOUND,
            "No explicit pull request creation was requested for this run.",
            "no_pull_request_creation",
        )
        .into_response(),
    }
}

async fn append_pull_request_creation_failure(
    run_store: &fabro_store::RunDatabase,
    run_id: &RunId,
    creation_id: fabro_types::PullRequestCreationId,
    error: String,
) -> anyhow::Result<()> {
    let event = workflow_event::Event::PullRequestFailed { error };
    workflow_event::append_event_if(run_store, run_id, &event, |projection| {
        projection
            .pull_request_creation
            .as_ref()
            .is_some_and(|creation| creation.id == creation_id && creation.is_pending())
    })
    .await?;
    Ok(())
}

pub(crate) async fn process_pull_request_creation(
    state: Arc<AppState>,
    run_id: RunId,
) -> anyhow::Result<()> {
    let _create_guard = lock_pull_request_create(&state.pull_request_create_locks, &run_id).await;
    let run_store = state.stores.runs.open_run(&run_id).await?;
    let run_state = run_store.state().await?;
    let Some(creation) = run_state
        .pull_request_creation
        .as_ref()
        .filter(|creation| creation.is_pending())
        .cloned()
    else {
        return Ok(());
    };
    if run_state.pull_request.is_some() {
        return Ok(());
    }

    let inputs = match RunPrInputs::extract(&run_state, creation.force) {
        Ok(inputs) => inputs,
        Err(err) => {
            return append_pull_request_creation_failure(
                &run_store,
                &run_id,
                creation.id,
                err.detail().to_string(),
            )
            .await;
        }
    };
    let creds = match load_server_github_credentials(state.as_ref()).await {
        Ok(creds) => creds,
        Err(err) => {
            return append_pull_request_creation_failure(
                &run_store,
                &run_id,
                creation.id,
                err.detail().to_string(),
            )
            .await;
        }
    };
    let github = match server_github_context(state.as_ref(), &creds) {
        Ok(github) => github,
        Err(err) => {
            return append_pull_request_creation_failure(
                &run_store,
                &run_id,
                creation.id,
                err.detail().to_string(),
            )
            .await;
        }
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
        run_state: Some(&run_state),
    };
    let shutdown = state.shutdown_token();
    let result = tokio::select! {
        () = shutdown.cancelled() => return Ok(()),
        result = time::timeout(PULL_REQUEST_CREATION_TIMEOUT, pull_request::open_pull_request(request)) => result,
    };
    let created_pull_request = match result {
        Ok(Ok(created)) => created,
        Ok(Err(err)) => {
            return append_pull_request_creation_failure(&run_store, &run_id, creation.id, err)
                .await;
        }
        Err(_) => {
            return append_pull_request_creation_failure(
                &run_store,
                &run_id,
                creation.id,
                "Pull request creation timed out after 10 minutes.".to_string(),
            )
            .await;
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
    workflow_event::append_event_if(&run_store, &run_id, &event, |projection| {
        projection.pull_request.is_none()
            && projection
                .pull_request_creation
                .as_ref()
                .is_some_and(|current| current.id == creation.id && current.is_pending())
    })
    .await?;
    Ok(())
}

async fn pending_pull_request_creation_run_ids(state: &AppState) -> anyhow::Result<Vec<RunId>> {
    let mut pending = state
        .stores
        .runs
        .list_cached_runs(&ListRunsQuery::default(), chrono::Utc::now())
        .await?
        .into_iter()
        .filter_map(|cached| {
            let creation = cached.projection.pull_request_creation.as_ref()?;
            (cached.projection.pull_request.is_none() && creation.is_pending())
                .then_some((cached.run_id, creation.requested_at))
        })
        .collect::<Vec<_>>();
    pending.sort_by_key(|(run_id, requested_at)| (*requested_at, *run_id));
    Ok(pending.into_iter().map(|(run_id, _)| run_id).collect())
}

pub(crate) fn spawn_pull_request_creation_supervisor(state: Arc<AppState>) -> JoinHandle<()> {
    tokio::spawn(
        run_pull_request_creation_supervisor(state)
            .instrument(tracing::info_span!("pull_request_creation_supervisor")),
    )
}

async fn run_pull_request_creation_supervisor(state: Arc<AppState>) {
    let shutdown = state.shutdown_token();
    let mut workers = JoinSet::new();
    let mut active = HashSet::new();
    let mut task_run_ids = std::collections::HashMap::new();
    let mut scan_requested = true;
    let mut scan_interval = time::interval(PULL_REQUEST_CREATION_SCAN_INTERVAL);
    scan_interval.set_missed_tick_behavior(time::MissedTickBehavior::Delay);

    loop {
        if scan_requested {
            match pending_pull_request_creation_run_ids(state.as_ref()).await {
                Ok(pending) => {
                    let available =
                        MAX_CONCURRENT_PULL_REQUEST_CREATIONS.saturating_sub(active.len());
                    let ready = pending
                        .into_iter()
                        .filter(|run_id| !active.contains(run_id))
                        .take(available)
                        .collect::<Vec<_>>();
                    for run_id in ready {
                        active.insert(run_id);
                        let task_state = Arc::clone(&state);
                        let handle = workers.spawn(
                            async move {
                                let result =
                                    process_pull_request_creation(task_state, run_id).await;
                                (run_id, result)
                            }
                            .instrument(
                                tracing::info_span!("pull_request_creation", run_id = %run_id),
                            ),
                        );
                        task_run_ids.insert(handle.id(), run_id);
                    }
                }
                Err(err) => {
                    tracing::warn!(error = %err, "Failed to scan queued pull request creations");
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
                    Some(Ok((task_id, (run_id, Ok(()))))) => {
                        task_run_ids.remove(&task_id);
                        active.remove(&run_id);
                        scan_requested = true;
                    }
                    Some(Ok((task_id, (run_id, Err(err))))) => {
                        task_run_ids.remove(&task_id);
                        active.remove(&run_id);
                        tracing::warn!(run_id = %run_id, error = %err, "Pull request creation worker failed");
                    }
                    Some(Err(err)) => {
                        if let Some(run_id) = task_run_ids.remove(&err.id()) {
                            active.remove(&run_id);
                            tracing::warn!(run_id = %run_id, error = %err, "Pull request creation worker stopped unexpectedly");
                        } else {
                            tracing::warn!(error = %err, "Pull request creation worker stopped unexpectedly");
                        }
                    }
                    None => {}
                }
            }
        }
    }

    while let Some(joined) = workers.join_next().await {
        if let Err(err) = joined {
            tracing::warn!(error = %err, "Pull request creation worker stopped during shutdown");
        }
    }
}

async fn link_run_pull_request(
    RequireRunScoped(id): RequireRunScoped,
    State(state): State<Arc<AppState>>,
    Json(body): Json<LinkRunPullRequestRequest>,
) -> Response {
    let _create_guard = lock_pull_request_create(&state.pull_request_create_locks, &id).await;
    let pull_request = match pull_request_record_from_link_request(&body) {
        Ok(record) => record,
        Err(err) => return err.into_response(),
    };
    let Ok(run_store) = state.stores.runs.open_run(&id).await else {
        return ApiError::not_found("Run not found.").into_response();
    };
    let event = workflow_event::Event::PullRequestLinked {
        pull_request: pull_request.clone(),
    };
    if let Err(err) = workflow_event::append_event(&run_store, &id, &event).await {
        return ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response();
    }

    Json(pull_request).into_response()
}

async fn unlink_run_pull_request(
    RequireRunScoped(id): RequireRunScoped,
    State(state): State<Arc<AppState>>,
) -> Response {
    let _create_guard = lock_pull_request_create(&state.pull_request_create_locks, &id).await;
    let Ok(run_store) = state.stores.runs.open_run(&id).await else {
        return ApiError::not_found("Run not found.").into_response();
    };
    let cached = match state.cached_run(&id).await {
        Ok(cached) => cached,
        Err(err) => return err.into_response(),
    };
    let Some(pull_request) = cached.projection.pull_request.clone() else {
        return ApiError::with_code(
            StatusCode::NOT_FOUND,
            format!("No pull request found in store. Create one first with: fabro pr create {id}"),
            "no_stored_record",
        )
        .into_response();
    };
    let event = workflow_event::Event::PullRequestUnlinked {
        pull_request: pull_request.clone(),
    };
    if let Err(err) = workflow_event::append_event(&run_store, &id, &event).await {
        return ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response();
    }

    Json(pull_request).into_response()
}

async fn get_run_pull_request(
    RequireRunScoped(id): RequireRunScoped,
    State(state): State<Arc<AppState>>,
) -> Response {
    let record = match load_pull_request_record(&state, &id).await {
        Ok(record) => record,
        Err(err) => return err.into_response(),
    };
    let (owner, repo, number) = github_coordinates_for_record(&record);
    let creds = match load_server_github_credentials(state.as_ref()).await {
        Ok(creds) => creds,
        Err(err) => {
            warn!(error = ?err, "Returning stored pull request without live GitHub details");
            return Json(unavailable_pull_request_response(
                record,
                fabro_types::PullRequestDetailsUnavailableReason::IntegrationUnavailable,
            ))
            .into_response();
        }
    };
    let github = match server_github_context(state.as_ref(), &creds) {
        Ok(github) => github,
        Err(err) => {
            warn!(error = ?err, "Returning stored pull request without live GitHub details");
            return Json(unavailable_pull_request_response(
                record,
                fabro_types::PullRequestDetailsUnavailableReason::IntegrationUnavailable,
            ))
            .into_response();
        }
    };

    match fabro_github::get_pull_request(&github, &owner, &repo, number).await {
        Ok(github) => Json(available_pull_request_response(record, github.into())).into_response(),
        Err(fabro_github::PullRequestApiError::NotFound { .. }) => {
            warn!("Returning stored pull request because GitHub no longer has the PR");
            Json(unavailable_pull_request_response(
                record,
                fabro_types::PullRequestDetailsUnavailableReason::NotFound,
            ))
            .into_response()
        }
        Err(err) => {
            warn!(error = %err, "Returning stored pull request without live GitHub details");
            Json(unavailable_pull_request_response(
                record,
                fabro_types::PullRequestDetailsUnavailableReason::FetchFailed,
            ))
            .into_response()
        }
    }
}

async fn merge_run_pull_request(
    RequireRunScoped(id): RequireRunScoped,
    State(state): State<Arc<AppState>>,
    Json(body): Json<MergeRunPullRequestRequest>,
) -> Response {
    let ctx = match load_pull_request_github_context(&state, &id).await {
        Ok(ctx) => ctx,
        Err(err) => return err.into_response(),
    };
    let github = match server_github_context(state.as_ref(), &ctx.creds) {
        Ok(github) => github,
        Err(err) => return err.into_response(),
    };

    match fabro_github::merge_pull_request(&github, &ctx.owner, &ctx.repo, ctx.number, body.method)
        .await
    {
        Ok(()) => Json(MergeRunPullRequestResponse {
            number:   i64::try_from(ctx.number)
                .expect("stored pull request number should fit in i64"),
            html_url: ctx.record.html_url(),
            method:   body.method,
        })
        .into_response(),
        Err(fabro_github::PullRequestApiError::NotFound { .. }) => {
            github_pull_request_not_found_error(ctx.number).into_response()
        }
        Err(err) => ApiError::new(StatusCode::BAD_GATEWAY, err.to_string()).into_response(),
    }
}

async fn close_run_pull_request(
    RequireRunScoped(id): RequireRunScoped,
    State(state): State<Arc<AppState>>,
) -> Response {
    let ctx = match load_pull_request_github_context(&state, &id).await {
        Ok(ctx) => ctx,
        Err(err) => return err.into_response(),
    };
    let github = match server_github_context(state.as_ref(), &ctx.creds) {
        Ok(github) => github,
        Err(err) => return err.into_response(),
    };

    match fabro_github::close_pull_request(&github, &ctx.owner, &ctx.repo, ctx.number).await {
        Ok(()) => Json(CloseRunPullRequestResponse {
            number:   i64::try_from(ctx.number)
                .expect("stored pull request number should fit in i64"),
            html_url: ctx.record.html_url(),
        })
        .into_response(),
        Err(fabro_github::PullRequestApiError::NotFound { .. }) => {
            github_pull_request_not_found_error(ctx.number).into_response()
        }
        Err(err) => ApiError::new(StatusCode::BAD_GATEWAY, err.to_string()).into_response(),
    }
}
