use std::path::{Path, PathBuf};

use fabro_api::types;
use fabro_config::{CliLayer, RunGoalLayer, RunLayer};
use fabro_manifest::{ManifestBuildInput, RunOverrideInput};
use fabro_tool::{ToolError, ToolResult, ValidatedCreateRunSpec};
use fabro_types::settings::interp::InterpString;

use crate::manifest_validation;

/// Build and validate a run manifest for the `fabro_run_create` tool.
///
/// Validation is structural. The caller is a client — an MCP server or a run
/// worker — whose catalog is its own, not the server's, so judging model and
/// provider availability here would reject workflows the server can run.
pub fn build_run_tool_manifest(
    spec: &ValidatedCreateRunSpec,
    cwd: &Path,
    user_settings_path: &Path,
) -> ToolResult<types::RunManifest> {
    let built = fabro_manifest::build_run_manifest(ManifestBuildInput {
        workflow:             PathBuf::from(&spec.workflow),
        cwd:                  cwd.to_path_buf(),
        run_overrides:        run_tool_run_overrides(spec),
        cli_overrides:        Some(CliLayer::default()),
        input_overrides:      spec.inputs.clone(),
        args:                 run_tool_manifest_args(spec),
        environment_defaults: fabro_environment::seeded_catalog_layer(),
        user_settings_path:   Some(user_settings_path.to_path_buf()),
    })
    .map_err(|err| ToolError::from_anyhow(&err))?;

    let mut validation =
        manifest_validation::validate_manifest(&RunLayer::default(), &built.manifest)
            .map_err(|err| ToolError::from_anyhow(&err))?;
    manifest_validation::promote_template_undefined_variables_to_errors(&mut validation);
    if !validation.ok {
        return Err(ToolError::message("workflow manifest validation failed"));
    }

    Ok(built.manifest)
}

pub fn run_tool_manifest_args(spec: &ValidatedCreateRunSpec) -> Option<types::ManifestArgs> {
    let mut input = spec
        .inputs
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>();
    input.sort();
    let mut label = spec
        .labels
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>();
    label.sort();

    let payload = types::ManifestArgs {
        auto_approve: spec.auto_approve.filter(|value| *value),
        dry_run: spec.dry_run.filter(|value| *value),
        input,
        label,
        model: spec.model.clone(),
        preserve_sandbox: spec.preserve_sandbox.filter(|value| *value),
        provider: spec.provider.clone(),
        environment: spec.environment.clone(),
        verbose: None,
    };
    (!fabro_manifest::manifest_args_is_empty(&payload)).then_some(payload)
}

pub fn run_tool_run_overrides(spec: &ValidatedCreateRunSpec) -> Option<RunLayer> {
    let mut run = fabro_manifest::build_run_overrides(RunOverrideInput {
        goal:             spec.goal.as_deref(),
        model:            spec.model.as_deref(),
        provider:         spec.provider.as_deref(),
        environment:      spec.environment.as_deref(),
        preserve_sandbox: spec.preserve_sandbox,
        dry_run:          spec.dry_run,
        auto_approve:     spec.auto_approve,
        labels:           spec.labels.clone(),
    });
    if let Some(goal_file) = spec.goal_file.as_ref() {
        run.goal = Some(RunGoalLayer::File {
            file: InterpString::parse(&goal_file.to_string_lossy()),
        });
    }
    (run.goal.is_some()
        || !run.metadata.is_empty()
        || run.model.is_some()
        || run.environment.is_some()
        || run.execution.is_some())
    .then_some(run)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use fabro_tool::CreateRunSpec;
    use serde_json::json;

    use super::*;

    fn create_run_spec(workflow: &str) -> ValidatedCreateRunSpec {
        ValidatedCreateRunSpec::try_from(CreateRunSpec {
            workflow:         workflow.to_string(),
            parent_id:        None,
            cwd:              None,
            goal:             None,
            goal_file:        None,
            inputs:           HashMap::new(),
            labels:           HashMap::new(),
            model:            None,
            provider:         None,
            environment:      None,
            dry_run:          None,
            auto_approve:     None,
            preserve_sandbox: None,
            start:            None,
        })
        .expect("create spec should validate")
    }

    /// The tool runs on a client, whose catalog is not the server's, so a
    /// server-owned model must reach the server rather than fail here.
    #[expect(
        clippy::disallowed_methods,
        reason = "sync test writes one workflow fixture before building the manifest"
    )]
    #[test]
    fn server_owned_provider_is_not_rejected_by_tool_manifest_validation() {
        let dir = tempfile::tempdir().expect("temp dir should be created");
        let workflow = dir.path().join("server-model.fabro");
        std::fs::write(
            &workflow,
            r#"digraph ServerModel {
                graph [goal="Use a server-owned model"]
                start [shape=Mdiamond]
                work [prompt="Do work", model="private-model", provider="server-only"]
                exit [shape=Msquare]
                start -> work -> exit
            }"#,
        )
        .expect("workflow fixture should be written");

        let manifest = build_run_tool_manifest(
            &create_run_spec(&workflow.to_string_lossy()),
            dir.path(),
            &dir.path().join("settings.toml"),
        )
        .expect("tool validation should leave provider availability to the server");

        let encoded = serde_json::to_string(&manifest).expect("manifest should serialize");
        assert!(
            encoded.contains("server-only"),
            "the authored provider should survive into the manifest: {encoded}"
        );
    }

    #[test]
    fn manifest_args_preserve_input_provenance() {
        let spec = ValidatedCreateRunSpec::try_from(CreateRunSpec {
            workflow:         "simple".to_string(),
            parent_id:        None,
            cwd:              None,
            goal:             None,
            goal_file:        None,
            inputs:           HashMap::from([
                ("count".to_string(), json!(3).into()),
                ("decision".to_string(), json!("approve").into()),
            ]),
            labels:           HashMap::new(),
            model:            None,
            provider:         None,
            environment:      None,
            dry_run:          None,
            auto_approve:     None,
            preserve_sandbox: None,
            start:            None,
        })
        .expect("create spec should validate");
        let args = run_tool_manifest_args(&spec).expect("input args should be present");

        assert_eq!(args.input, vec![r"count=3", r#"decision="approve""#]);
    }

    #[expect(
        clippy::disallowed_methods,
        reason = "test asserts the raw template source"
    )]
    #[test]
    fn run_overrides_preserve_goal_file_as_file_goal() {
        let spec = ValidatedCreateRunSpec::try_from(CreateRunSpec {
            workflow:         "implement-plan".to_string(),
            parent_id:        None,
            cwd:              None,
            goal:             None,
            goal_file:        Some(PathBuf::from("plans/ship-it.md")),
            inputs:           HashMap::new(),
            labels:           HashMap::new(),
            model:            None,
            provider:         None,
            environment:      None,
            dry_run:          None,
            auto_approve:     None,
            preserve_sandbox: None,
            start:            None,
        })
        .expect("create spec with goal_file should validate");

        let run = run_tool_run_overrides(&spec).expect("goal_file should produce run overrides");
        let Some(fabro_config::RunGoalLayer::File { file }) = run.goal else {
            panic!("goal_file should become a file goal override");
        };
        assert_eq!(file.as_source(), "plans/ship-it.md");
    }
}
