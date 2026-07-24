use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use fabro_model::{Catalog, ProviderId};
use fabro_types::WorkflowSettings;

use super::create::{preprocess_and_validate, template_context};
use super::source::{ResolveWorkflowInput, WorkflowInput, resolve_workflow};
use crate::error::Error;
use crate::operations::RenderMode;
use crate::pipeline::Validated;
use crate::transforms::Transform;

pub struct ValidateInput {
    pub workflow:          WorkflowInput,
    pub settings:          WorkflowSettings,
    /// Run-scoped variables (`{{ vars.* }}`) available to prompts and goals.
    /// Empty for offline/CLI validation.
    pub vars:              HashMap<String, String>,
    pub cwd:               PathBuf,
    pub custom_transforms: Vec<Box<dyn Transform>>,
    pub catalog:           Arc<Catalog>,
}

/// Parse, transform, and validate a DOT source string.
///
/// Returns `Validated` even when validation produced errors. Call
/// `validated.raise_on_errors()` if the caller wants to fail fast.
pub fn validate(input: ValidateInput) -> Result<Validated, Error> {
    let eligible_providers = input
        .catalog
        .all_provider_ids()
        .into_iter()
        .collect::<Vec<_>>();
    validate_with_eligible_providers(input, &eligible_providers, false)
}

/// Parse, transform, and validate, resolving models against the ready
/// providers first and falling back to the full catalog only for
/// provider-readiness selection failures.
pub fn validate_with_ready_providers(
    input: ValidateInput,
    ready_providers: &[ProviderId],
) -> Result<Validated, Error> {
    validate_with_eligible_providers(input, ready_providers, true)
}

fn validate_with_eligible_providers(
    input: ValidateInput,
    eligible_providers: &[ProviderId],
    catalog_fallback: bool,
) -> Result<Validated, Error> {
    let resolved = resolve_workflow(ResolveWorkflowInput {
        workflow: input.workflow,
        settings: input.settings,
        cwd:      input.cwd,
    })
    .map_err(|err| Error::Parse(err.to_string()))?;

    preprocess_and_validate(
        &resolved.raw_source,
        resolved
            .dot_path
            .as_ref()
            .map(|path| path.display().to_string()),
        resolved.current_dir,
        resolved.file_resolver,
        input.custom_transforms,
        template_context(Some(&resolved.settings), input.vars),
        resolved.goal_override.as_deref(),
        RenderMode::Structural,
        resolved
            .settings
            .run
            .model
            .provider
            .as_deref()
            .filter(|provider| !provider.is_empty())
            .map(fabro_model::ProviderId::new),
        eligible_providers,
        catalog_fallback,
        &input.catalog,
    )
}
