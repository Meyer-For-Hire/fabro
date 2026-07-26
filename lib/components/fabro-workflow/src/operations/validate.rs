use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use fabro_model::{Catalog, ProviderId};
use fabro_types::WorkflowSettings;

use super::create::{
    preprocess_and_validate, preprocess_and_validate_structural, template_context,
};
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
}

/// Parse, transform, and structurally validate a DOT source string without a
/// model catalog.
///
/// Returns `Validated` even when validation produced errors. Call
/// `validated.raise_on_errors()` if the caller wants to fail fast.
pub fn validate(input: ValidateInput) -> Result<Validated, Error> {
    let ValidateInput {
        workflow,
        settings,
        vars,
        cwd,
        custom_transforms,
    } = input;
    let resolved = resolve_workflow(ResolveWorkflowInput {
        workflow,
        settings,
        cwd,
    })
    .map_err(|err| Error::Parse(err.to_string()))?;

    preprocess_and_validate_structural(
        &resolved.raw_source,
        resolved
            .dot_path
            .as_ref()
            .map(|path| path.display().to_string()),
        resolved.current_dir,
        resolved.file_resolver,
        custom_transforms,
        template_context(Some(&resolved.settings), vars),
        resolved.goal_override.as_deref(),
        RenderMode::Structural,
    )
}

/// Parse, transform, and validate a DOT source string against `catalog`.
pub fn validate_with_catalog(
    input: ValidateInput,
    catalog: &Arc<Catalog>,
) -> Result<Validated, Error> {
    let eligible_providers = catalog.all_provider_ids().into_iter().collect::<Vec<_>>();
    validate_with_eligible_providers(input, catalog, &eligible_providers, false)
}

/// Parse, transform, and validate, resolving models against the ready
/// providers first and falling back to the full catalog only for
/// provider-readiness selection failures.
pub fn validate_with_ready_providers(
    input: ValidateInput,
    catalog: &Arc<Catalog>,
    ready_providers: &[ProviderId],
) -> Result<Validated, Error> {
    validate_with_eligible_providers(input, catalog, ready_providers, true)
}

fn validate_with_eligible_providers(
    input: ValidateInput,
    catalog: &Arc<Catalog>,
    eligible_providers: &[ProviderId],
    catalog_fallback: bool,
) -> Result<Validated, Error> {
    let ValidateInput {
        workflow,
        settings,
        vars,
        cwd,
        custom_transforms,
    } = input;
    let resolved = resolve_workflow(ResolveWorkflowInput {
        workflow,
        settings,
        cwd,
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
        custom_transforms,
        template_context(Some(&resolved.settings), vars),
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
        catalog,
    )
}
