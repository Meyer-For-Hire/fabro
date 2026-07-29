use std::collections::HashMap;

use anyhow::Result;
use fabro_api::types;
use fabro_config::RunLayer;
use fabro_workflow::pipeline::TEMPLATE_UNDEFINED_VARIABLE_RULE;

use crate::run_manifest;

/// Validate a manifest without a model catalog.
///
/// Every caller is a client — the CLI, an MCP server, a run worker — and a
/// client's catalog is its own, not the server's. Judging model and provider
/// availability here would reject workflows the server can run, so that is
/// left to the server on create.
pub fn validate_manifest(
    manifest_run_defaults: &RunLayer,
    manifest: &types::RunManifest,
) -> Result<types::ValidateResponse> {
    let prepared = run_manifest::prepare_manifest_with_environment_defaults(
        manifest_run_defaults,
        &fabro_environment::seeded_catalog_layer(),
        &HashMap::new(),
        manifest,
    )?;
    let validated = run_manifest::validate_prepared_manifest_structural(&prepared)
        .map_err(anyhow::Error::new)?;
    Ok(run_manifest::validate_response(&prepared, &validated))
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
