use std::sync::Arc;

use super::pull_request::{AutoMergeOptions, OpenPullRequestRequest, maybe_open_pull_request};
use super::types::{Concluded, PublishOptions, PublishOutcome, Published};
use crate::error::Error;
use crate::event::Event;
use crate::outcome::StageOutcome;

/// PUBLISH phase: push the final run commit and, when configured, open a pull
/// request.
///
/// Publish is always present in the pipeline. It becomes a no-op when the run
/// did not succeed, is a dry run, or has no remote branch configured.
pub async fn publish(concluded: Concluded, options: &PublishOptions) -> Published {
    let publish_outcome = publish_inner(&concluded, options).await;
    let Concluded {
        outcome,
        conclusion,
        artifact_count,
        graph: _,
        run_options,
        services,
    } = concluded;

    Published {
        execution_outcome: outcome,
        publish_outcome,
        conclusion,
        artifact_count,
        run_options,
        services,
    }
}

async fn publish_inner(
    concluded: &Concluded,
    options: &PublishOptions,
) -> Result<PublishOutcome, Error> {
    let successful_execution = concluded.outcome.as_ref().is_ok_and(|outcome| {
        matches!(
            outcome.status,
            StageOutcome::Succeeded | StageOutcome::PartiallySucceeded
        )
    });
    if !successful_execution || concluded.run_options.dry_run_enabled() {
        return Ok(PublishOutcome::NotRequested);
    }

    let pull_request_requested = options.pr_config.is_some();
    let Some(origin_url) = options
        .origin_url
        .as_deref()
        .filter(|origin| !origin.trim().is_empty())
    else {
        if pull_request_requested {
            return Err(pull_request_error(
                concluded,
                "pull request creation requires a GitHub origin URL",
            ));
        }
        return Ok(PublishOutcome::NotRequested);
    };
    let Some(run_branch) = concluded.run_options.run_branch() else {
        if pull_request_requested {
            return Err(pull_request_error(
                concluded,
                "pull request creation requires a run branch",
            ));
        }
        return Ok(PublishOutcome::NotRequested);
    };
    if !concluded.run_options.settings.run.run_branch.push {
        if pull_request_requested {
            return Err(pull_request_error(
                concluded,
                "pull request creation requires run branch pushing",
            ));
        }
        return Ok(PublishOutcome::NotRequested);
    }

    let final_sha = concluded
        .conclusion
        .final_git_commit_sha
        .as_deref()
        .ok_or_else(|| Error::publish("cannot publish a run without a final git commit SHA"))?;
    let refspec = format!("refs/heads/{run_branch}:refs/heads/{run_branch}");
    match concluded.services.sandbox.git_push_ref(&refspec).await {
        Ok(()) => {
            concluded.services.emitter.emit(&Event::GitPush {
                branch:           run_branch.to_string(),
                success:          true,
                exec_output_tail: None,
            });
        }
        Err(error) => {
            let exec_output_tail = fabro_sandbox::default_redacted_output_tail(&error);
            concluded.services.emitter.emit(&Event::GitPush {
                branch:           run_branch.to_string(),
                success:          false,
                exec_output_tail: exec_output_tail.clone(),
            });
            return Err(Error::publish_with_source_and_exec_output_tail(
                format!("failed to push final commit {final_sha} to branch '{run_branch}'"),
                error,
                exec_output_tail,
            ));
        }
    }

    let diff = concluded
        .conclusion
        .diff
        .patch
        .as_deref()
        .unwrap_or_default();
    let Some(pr_config) = options.pr_config.as_ref() else {
        return Ok(PublishOutcome::Published {
            pushed_branch: run_branch.to_string(),
            pr_url:        None,
        });
    };
    if diff.trim().is_empty() {
        return Ok(PublishOutcome::NoChanges {
            pushed_branch: run_branch.to_string(),
        });
    }

    let base_branch = concluded
        .run_options
        .base_branch
        .as_deref()
        .ok_or_else(|| {
            pull_request_error(concluded, "pull request creation requires a base branch")
        })?;
    let credentials = options.github_app.as_ref().ok_or_else(|| {
        pull_request_error(
            concluded,
            "pull request creation requires GitHub credentials",
        )
    })?;
    let auto_merge = pr_config.auto_merge.then_some(AutoMergeOptions {
        merge_strategy: pr_config.merge_strategy,
    });
    let github_base_url = fabro_github::github_api_base_url();

    let created = maybe_open_pull_request(OpenPullRequestRequest {
        github: fabro_github::GitHubContext::new(credentials, &github_base_url),
        origin_url,
        base_branch,
        head_branch: run_branch,
        expected_head_sha: final_sha,
        goal: concluded.graph.goal(),
        diff,
        model: &options.model,
        draft: pr_config.draft,
        auto_merge,
        run_store: &concluded.services.run_store,
        llm_source: concluded.services.llm_source.as_ref(),
        catalog: Arc::clone(&concluded.services.catalog),
        conclusion: Some(&concluded.conclusion),
        run_state: None,
    })
    .await
    .map_err(|error| {
        concluded.services.emitter.emit(&Event::PullRequestFailed {
            error: error.clone(),
        });
        Error::publish_with_source("failed to create pull request", anyhow::anyhow!(error))
    })?
    .ok_or_else(|| {
        pull_request_error(
            concluded,
            "pull request creation found no changes after the stored diff was checked",
        )
    })?;

    concluded
        .services
        .emitter
        .emit(&Event::pull_request_created(
            &created.link,
            &created.base_branch,
            &created.head_branch,
            &created.head_sha,
            &created.title,
            pr_config.draft,
        ));

    Ok(PublishOutcome::Published {
        pushed_branch: run_branch.to_string(),
        pr_url:        Some(created.link.html_url()),
    })
}

fn pull_request_error(concluded: &Concluded, message: &str) -> Error {
    concluded.services.emitter.emit(&Event::PullRequestFailed {
        error: message.to_string(),
    });
    Error::publish(message)
}
