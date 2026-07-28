use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use fabro_api::types;
use fabro_config::RunLayer;
use fabro_model::Catalog;
use fabro_workflow::pipeline::TEMPLATE_UNDEFINED_VARIABLE_RULE;

use crate::run_manifest;

/// Validate a manifest without a model catalog. Model and provider
/// availability is deferred to the server, which owns the catalog.
pub fn validate_manifest(
    manifest_run_defaults: &RunLayer,
    manifest: &types::RunManifest,
) -> Result<types::ValidateResponse> {
    let prepared = prepare(manifest_run_defaults, manifest)?;
    let validated = run_manifest::validate_prepared_manifest_structural(&prepared)
        .map_err(anyhow::Error::new)?;
    Ok(run_manifest::validate_response(&prepared, &validated))
}

/// Validate a manifest including the catalog-backed model and provider rules.
pub fn validate_manifest_with_catalog(
    manifest_run_defaults: &RunLayer,
    manifest: &types::RunManifest,
    catalog: Arc<Catalog>,
) -> Result<types::ValidateResponse> {
    let prepared = prepare(manifest_run_defaults, manifest)?;
    let validated =
        run_manifest::validate_prepared_manifest(&prepared, catalog).map_err(anyhow::Error::new)?;
    Ok(run_manifest::validate_response(&prepared, &validated))
}

fn prepare(
    manifest_run_defaults: &RunLayer,
    manifest: &types::RunManifest,
) -> Result<run_manifest::PreparedManifest> {
    run_manifest::prepare_manifest_with_environment_defaults(
        manifest_run_defaults,
        &fabro_environment::seeded_catalog_layer(),
        &HashMap::new(),
        manifest,
    )
}

pub fn promote_template_undefined_variables_to_errors(response: &mut types::ValidateResponse) {
    let mut promoted = false;
    for diagnostic in &mut response.workflow.diagnostics {
        if diagnostic.rule == TEMPLATE_UNDEFINED_VARIABLE_RULE {
            diagnostic.severity = types::WorkflowDiagnosticSeverity::Error;
            promoted = true;
        }
    }
    if promoted {
        response.ok = false;
    }
}
