use std::path::Path;

use fabro_api::types;
use fabro_server::run_tool_manifest;
use fabro_tool::{RunManifestBuilder, ToolResult, ValidatedCreateRunSpec};

#[derive(Default)]
pub(crate) struct McpRunManifestBuilder;

impl RunManifestBuilder for McpRunManifestBuilder {
    fn build_run_manifest(
        &self,
        spec: &ValidatedCreateRunSpec,
        cwd: &Path,
        user_settings_path: &Path,
    ) -> ToolResult<types::RunManifest> {
        run_tool_manifest::build_run_tool_manifest(spec, cwd, user_settings_path)
    }
}
