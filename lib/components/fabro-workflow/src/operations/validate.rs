use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use fabro_model::{Catalog, ProviderId};
use fabro_types::WorkflowSettings;

use super::create::{configured_default_provider, preprocess_and_validate, template_context};
use super::source::{ResolveWorkflowInput, WorkflowInput, resolve_workflow};
use crate::error::Error;
use crate::operations::RenderMode;
use crate::pipeline::{ModelResolutionOptions, TransformOptions, Validated};
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

/// Which providers catalog-backed model resolution may select from. The
/// workflow's own default provider is read from the resolved settings, so it
/// is not part of the caller's request.
struct CatalogScope<'a> {
    catalog:            &'a Arc<Catalog>,
    eligible_providers: HashSet<ProviderId>,
    /// Fall back to the full catalog when the eligible providers cannot
    /// supply a requested model, instead of erroring.
    catalog_fallback:   bool,
}

/// Parse, transform, and structurally validate a DOT source string without a
/// model catalog. Model and provider availability is left to the caller that
/// owns a catalog — typically the server.
///
/// Returns `Validated` even when validation produced errors. Call
/// `validated.raise_on_errors()` if the caller wants to fail fast.
pub fn validate(input: ValidateInput) -> Result<Validated, Error> {
    validate_in_scope(input, None)
}

/// Parse, transform, and validate a DOT source string against `catalog`.
pub fn validate_with_catalog(
    input: ValidateInput,
    catalog: &Arc<Catalog>,
) -> Result<Validated, Error> {
    validate_in_scope(
        input,
        Some(CatalogScope {
            catalog,
            eligible_providers: catalog.all_provider_ids(),
            catalog_fallback: false,
        }),
    )
}

/// Parse, transform, and validate, resolving models against the ready
/// providers first and falling back to the full catalog only for
/// provider-readiness selection failures.
pub fn validate_with_ready_providers(
    input: ValidateInput,
    catalog: &Arc<Catalog>,
    ready_providers: &[ProviderId],
) -> Result<Validated, Error> {
    validate_in_scope(
        input,
        Some(CatalogScope {
            catalog,
            eligible_providers: ready_providers.iter().cloned().collect(),
            catalog_fallback: true,
        }),
    )
}

fn validate_in_scope(
    input: ValidateInput,
    scope: Option<CatalogScope<'_>>,
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

    let model_resolution = scope.map(|scope| ModelResolutionOptions {
        catalog:            Arc::clone(scope.catalog),
        default_provider:   configured_default_provider(&resolved.settings),
        eligible_providers: scope.eligible_providers,
        catalog_fallback:   scope.catalog_fallback,
    });

    preprocess_and_validate(
        &resolved.raw_source,
        resolved.goal_override.as_deref(),
        &TransformOptions {
            current_dir: resolved.current_dir,
            file_resolver: resolved.file_resolver,
            template_context: template_context(Some(&resolved.settings), vars),
            source_name: resolved
                .dot_path
                .as_ref()
                .map(|path| path.display().to_string()),
            render_mode: RenderMode::Structural,
            custom_transforms,
            model_resolution,
        },
    )
}
