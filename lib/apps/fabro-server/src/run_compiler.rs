use std::collections::HashMap;
use std::error::Error as StdError;
use std::path::PathBuf;
use std::sync::Arc;

use fabro_config::parse::{self, SettingsSource};
use fabro_config::{
    CliLayer, EnvironmentDockerfileLayer, EnvironmentImageLayer, EnvironmentLayer, MergeMap,
    RunLayer, SettingsLayer, WorkflowSettingsBuilder,
};
use fabro_model::{Catalog, ModelSelectionError, ProviderId};
use fabro_types::settings::AmbiguousModelRef;
use fabro_types::settings::interp::{InterpString, ResolveError};
use fabro_types::settings::run::{McpServerSettings, RunGoal};
use fabro_types::{
    AutomationRef, GitContext, ManifestPath, RunId, RunProvenance, WorkflowSettings,
};
use fabro_util::workspace_glob::{WorkspaceGlob, WorkspaceGlobError};
use fabro_workflow::Error as WorkflowError;
use fabro_workflow::operations::{
    self, CompiledRun, CreateRunCompileInput, CreateRunPersistenceInput,
    CreateRunPersistenceMetadata, MaterializedRun, WorkflowInput,
};
use fabro_workflow::workflow_bundle::{BundledWorkflow, WorkflowBundle};
use tokio::task;

/// One project settings source already normalized into the manifest path
/// namespace used by the workflow bundle.
#[derive(Clone, Debug)]
pub(crate) struct ProjectSettingsSource {
    pub(crate) path: ManifestPath,
    pub(crate) toml: String,
}

/// Transport-neutral inputs for compiling one submitted run.
///
/// IDs, title, git metadata, and provenance are resolved by the caller. This
/// boundary owns only source normalization, settings resolution, workflow
/// compilation, materialization, and persistence-input assembly.
#[derive(Clone, Debug)]
pub(crate) struct RawRunCompilerInput {
    pub(crate) workflow_bundle: WorkflowBundle,
    pub(crate) entrypoint: ManifestPath,
    pub(crate) cwd: PathBuf,
    pub(crate) server_run_defaults: RunLayer,
    pub(crate) server_environment_defaults: MergeMap<EnvironmentLayer>,
    pub(crate) server_mcp_catalog: HashMap<String, McpServerSettings>,
    pub(crate) project_settings: Vec<ProjectSettingsSource>,
    pub(crate) user_toml: Vec<String>,
    pub(crate) run_overrides: Option<RunLayer>,
    pub(crate) cli_overrides: Option<CliLayer>,
    pub(crate) input_overrides: HashMap<String, toml::Value>,
    pub(crate) inline_goal_override: Option<String>,
    pub(crate) vars: HashMap<String, String>,
    pub(crate) run_id: Option<RunId>,
    pub(crate) title: Option<String>,
    pub(crate) parent_id: Option<RunId>,
    pub(crate) git: Option<GitContext>,
    pub(crate) storage_root: PathBuf,
    pub(crate) configured_providers: Vec<ProviderId>,
    pub(crate) workflow_slug: Option<String>,
    pub(crate) provenance: RunProvenance,
    pub(crate) web_url: Option<String>,
    pub(crate) submitted_manifest_bytes: Option<Vec<u8>>,
    pub(crate) automation: Option<AutomationRef>,
}

#[derive(Clone, Debug)]
struct RunMetadata {
    run_id: Option<RunId>,
    title: Option<String>,
    parent_id: Option<RunId>,
    git: Option<GitContext>,
    storage_root: PathBuf,
    workflow_slug: Option<String>,
    provenance: RunProvenance,
    web_url: Option<String>,
    submitted_manifest_bytes: Option<Vec<u8>>,
    automation: Option<AutomationRef>,
}

/// Stage-one output: the selected bundled workflow and all client settings
/// sources have been parsed and normalized, but no settings have been layered.
pub(crate) struct NormalizedRun {
    workflow_bundle: WorkflowBundle,
    entrypoint: ManifestPath,
    workflow: BundledWorkflow,
    workflow_layer: Option<SettingsLayer>,
    project_layers: Vec<SettingsLayer>,
    user_toml: Vec<String>,
    cwd: PathBuf,
    server_run_defaults: RunLayer,
    server_environment_defaults: MergeMap<EnvironmentLayer>,
    server_mcp_catalog: HashMap<String, McpServerSettings>,
    run_overrides: Option<RunLayer>,
    cli_overrides: Option<CliLayer>,
    input_overrides: HashMap<String, toml::Value>,
    inline_goal_override: Option<String>,
    vars: HashMap<String, String>,
    configured_providers: Vec<ProviderId>,
    metadata: RunMetadata,
}

/// Settings-layered output. Variable substitution is intentionally separate
/// so callers can snapshot variables after source/settings preparation, as the
/// create handler historically does.
pub(crate) struct LayeredRun {
    workflow_bundle:      WorkflowBundle,
    entrypoint:           ManifestPath,
    workflow:             BundledWorkflow,
    settings:             WorkflowSettings,
    cwd:                  PathBuf,
    vars:                 HashMap<String, String>,
    configured_providers: Vec<ProviderId>,
    metadata:             RunMetadata,
}

impl LayeredRun {
    pub(crate) fn with_vars(mut self, vars: HashMap<String, String>) -> Self {
        self.vars = vars;
        self
    }
}

/// Settings-resolved stage output. Callers may inspect this before policy
/// checks, then move it into [`compile_graph`] after those checks pass.
pub(crate) struct PreparedRun {
    workflow_bundle:      WorkflowBundle,
    entrypoint:           ManifestPath,
    workflow:             BundledWorkflow,
    settings:             WorkflowSettings,
    cwd:                  PathBuf,
    vars:                 HashMap<String, String>,
    configured_providers: Vec<ProviderId>,
    metadata:             RunMetadata,
}

impl PreparedRun {
    pub(crate) fn resolve_run_id(mut self) -> (Self, RunId) {
        let run_id = self.metadata.run_id.unwrap_or_default();
        self.metadata.run_id = Some(run_id);
        (self, run_id)
    }

    pub(crate) fn with_web_url(mut self, web_url: Option<String>) -> Self {
        self.metadata.web_url = web_url;
        self
    }

    pub(crate) fn with_configured_providers(
        mut self,
        configured_providers: Vec<ProviderId>,
    ) -> Self {
        self.configured_providers = configured_providers;
        self
    }

    pub(crate) fn settings(&self) -> &WorkflowSettings {
        &self.settings
    }

    pub(crate) fn parent_id(&self) -> Option<RunId> {
        self.metadata.parent_id
    }
}

/// Graph-compiled stage output, retaining the metadata needed by later pure
/// assembly.
pub(crate) struct GraphCompiledRun {
    compiled:   CompiledRun,
    entrypoint: ManifestPath,
    metadata:   RunMetadata,
}

impl GraphCompiledRun {
    #[cfg(test)]
    pub(crate) fn compiled(&self) -> &CompiledRun {
        &self.compiled
    }
}

/// Materialized stage output ready for pure persistence-input assembly.
pub(crate) struct PersistenceReadyRun {
    materialized: MaterializedRun,
    entrypoint:   ManifestPath,
    metadata:     RunMetadata,
}

/// Complete output of this boundary.
pub(crate) struct RunCompilerOutput {
    persistence_input: CreateRunPersistenceInput,
    entrypoint:        ManifestPath,
}

impl RunCompilerOutput {
    #[cfg(test)]
    pub(crate) fn persistence_input(&self) -> &CreateRunPersistenceInput {
        &self.persistence_input
    }

    #[cfg(test)]
    pub(crate) fn entrypoint(&self) -> &ManifestPath {
        &self.entrypoint
    }

    pub(crate) fn into_parts(self) -> (CreateRunPersistenceInput, ManifestPath) {
        (self.persistence_input, self.entrypoint)
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum RunCompilerError {
    #[error("invalid run source: {source}")]
    InvalidSource {
        #[source]
        source: InvalidSourceError,
    },

    #[error("invalid run settings: {source}")]
    InvalidSettings {
        #[source]
        source: Box<InvalidSettingsError>,
    },

    #[error("run config variable interpolation failed: {source}")]
    VariableInterpolation {
        #[source]
        source: VariableInterpolationError,
    },

    #[error("workflow validation or parse failed: {source}")]
    ValidationOrParse {
        #[source]
        source: WorkflowError,
    },

    #[error("model selection failed: {source}")]
    ModelSelection {
        #[source]
        source: ModelSelectionError,
    },

    #[error("model reference failed: {source}")]
    ModelReference {
        #[source]
        source: AmbiguousModelRef,
    },

    #[error("{context}")]
    Internal {
        context: &'static str,
        #[source]
        source:  Box<dyn StdError + Send + Sync>,
    },
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum InvalidSourceError {
    #[error("bundle entrypoint {entrypoint} is missing from the workflow bundle")]
    MissingEntrypoint { entrypoint: ManifestPath },

    #[error("unsupported dockerfile reference {reference:?} in {config_path}")]
    UnsupportedDockerfileReference {
        config_path: ManifestPath,
        reference:   String,
    },

    #[error("bundled dockerfile {dockerfile_path} referenced by {config_path} is missing")]
    MissingDockerfile {
        config_path:     ManifestPath,
        dockerfile_path: ManifestPath,
    },
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum InvalidSettingsError {
    #[error("failed to parse {kind} settings at {path}")]
    Parse {
        kind:   &'static str,
        path:   ManifestPath,
        #[source]
        source: Box<dyn StdError + Send + Sync>,
    },

    #[error("failed to parse user settings: {source}")]
    User {
        #[source]
        source: fabro_config::Error,
    },

    #[error("failed to resolve layered workflow settings: {source}")]
    Resolve {
        #[source]
        source: fabro_config::ResolveErrors,
    },
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum VariableInterpolationError {
    #[error(transparent)]
    Interpolation(#[from] ResolveError),

    #[error("run.artifacts.include[{index}]: {source}")]
    ArtifactGlob {
        index:  usize,
        #[source]
        source: WorkspaceGlobError,
    },
}

pub(crate) type Result<T> = std::result::Result<T, RunCompilerError>;

/// Normalize the bundle entrypoint and parse workflow/project settings while
/// resolving Dockerfile references against the selected workflow's files.
pub(crate) fn normalize_source(input: RawRunCompilerInput) -> Result<NormalizedRun> {
    let RawRunCompilerInput {
        workflow_bundle,
        entrypoint,
        cwd,
        server_run_defaults,
        server_environment_defaults,
        server_mcp_catalog,
        project_settings,
        user_toml,
        run_overrides,
        cli_overrides,
        input_overrides,
        inline_goal_override,
        vars,
        run_id,
        title,
        parent_id,
        git,
        storage_root,
        configured_providers,
        workflow_slug,
        provenance,
        web_url,
        submitted_manifest_bytes,
        automation,
    } = input;
    let mut workflow = workflow_bundle
        .workflow(&entrypoint)
        .cloned()
        .ok_or_else(|| RunCompilerError::InvalidSource {
            source: InvalidSourceError::MissingEntrypoint {
                entrypoint: entrypoint.clone(),
            },
        })?;
    workflow.path = entrypoint.clone();

    let workflow_layer = workflow
        .config
        .as_ref()
        .map(|config| {
            parse_settings_layer(
                &config.source,
                &config.path,
                &workflow.files,
                SettingsSource::Workflow,
                "workflow",
            )
        })
        .transpose()?;
    let project_layers = project_settings
        .into_iter()
        .map(|project| {
            parse_settings_layer(
                &project.toml,
                &project.path,
                &workflow.files,
                SettingsSource::Project,
                "project",
            )
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(NormalizedRun {
        workflow_bundle,
        entrypoint,
        workflow,
        workflow_layer,
        project_layers,
        user_toml,
        cwd,
        server_run_defaults,
        server_environment_defaults,
        server_mcp_catalog,
        run_overrides,
        cli_overrides,
        input_overrides,
        inline_goal_override,
        vars,
        configured_providers,
        metadata: RunMetadata {
            run_id,
            title,
            parent_id,
            git,
            storage_root,
            workflow_slug,
            provenance,
            web_url,
            submitted_manifest_bytes,
            automation,
        },
    })
}

/// Layer settings and apply the submitted input and goal overrides.
pub(crate) fn layer_settings(normalized: NormalizedRun) -> Result<LayeredRun> {
    let NormalizedRun {
        workflow_bundle,
        entrypoint,
        workflow,
        workflow_layer,
        project_layers,
        user_toml,
        cwd,
        server_run_defaults,
        server_environment_defaults,
        server_mcp_catalog,
        run_overrides,
        cli_overrides,
        input_overrides,
        inline_goal_override,
        vars,
        configured_providers,
        metadata,
    } = normalized;
    let mut builder = WorkflowSettingsBuilder::new()
        .server_manifest_defaults(server_run_defaults, server_environment_defaults)
        .server_mcp_catalog(server_mcp_catalog);
    if let Some(run) = run_overrides {
        builder = builder.run_overrides(run);
    }
    if let Some(cli) = cli_overrides {
        builder = builder.cli_overrides(cli);
    }
    if let Some(layer) = workflow_layer {
        builder = builder.workflow_layer(layer);
    }
    for layer in project_layers {
        builder = builder.project_layer(layer);
    }
    for source in user_toml {
        builder =
            builder
                .user_toml(&source)
                .map_err(|source| RunCompilerError::InvalidSettings {
                    source: Box::new(InvalidSettingsError::User { source }),
                })?;
    }
    let mut settings = builder
        .build()
        .map_err(|source| RunCompilerError::InvalidSettings {
            source: Box::new(InvalidSettingsError::Resolve { source }),
        })?;
    settings.run.inputs.extend(input_overrides);
    if let Some(goal) = inline_goal_override {
        settings.run.goal = Some(RunGoal::Inline(InterpString::parse(&goal)));
    }

    Ok(LayeredRun {
        workflow_bundle,
        entrypoint,
        workflow,
        settings,
        cwd,
        vars,
        configured_providers,
        metadata,
    })
}

/// Apply a run-variable snapshot and validate the resulting artifact globs.
pub(crate) fn apply_run_variables(mut layered: LayeredRun) -> Result<PreparedRun> {
    substitute_variables(&layered.vars, &mut layered.settings)?;
    Ok(PreparedRun {
        workflow_bundle:      layered.workflow_bundle,
        entrypoint:           layered.entrypoint,
        workflow:             layered.workflow,
        settings:             layered.settings,
        cwd:                  layered.cwd,
        vars:                 layered.vars,
        configured_providers: layered.configured_providers,
        metadata:             layered.metadata,
    })
}

/// Run stage one and the settings/variables portion of stage two. This is the
/// convenient boundary for callers that already own a variable snapshot.
#[cfg(test)]
pub(crate) fn prepare_run(input: RawRunCompilerInput) -> Result<PreparedRun> {
    apply_run_variables(layer_settings(normalize_source(input)?)?)
}

/// Compile and validate the graph on Tokio's blocking pool.
pub(crate) async fn compile_graph(
    prepared: PreparedRun,
    catalog: Arc<Catalog>,
) -> Result<GraphCompiledRun> {
    let PreparedRun {
        workflow_bundle,
        entrypoint,
        workflow,
        settings,
        cwd,
        vars,
        configured_providers,
        metadata,
    } = prepared;
    let compile_input = CreateRunCompileInput {
        workflow: WorkflowInput::Bundled(workflow),
        settings,
        vars,
        cwd,
        workflow_path: Some(entrypoint.clone()),
        workflow_bundle: Some(workflow_bundle),
        configured_providers,
    };
    let compiled =
        task::spawn_blocking(move || operations::compile_create_run(compile_input, catalog))
            .await
            .map_err(|source| RunCompilerError::Internal {
                context: "workflow compilation failed",
                source:  Box::new(WorkflowError::engine_with_source(
                    "workflow create task failed",
                    source,
                )),
            })?
            .map_err(classify_workflow_error)?;

    Ok(GraphCompiledRun {
        compiled,
        entrypoint,
        metadata,
    })
}

/// Materialize run-level model settings on Tokio's blocking pool.
pub(crate) async fn materialize_run(
    compiled: GraphCompiledRun,
    catalog: Arc<Catalog>,
) -> Result<PersistenceReadyRun> {
    let GraphCompiledRun {
        compiled,
        entrypoint,
        metadata,
    } = compiled;
    let materialized = task::spawn_blocking(move || {
        operations::materialize_create_run(compiled, catalog.as_ref())
    })
    .await
    .map_err(|source| RunCompilerError::Internal {
        context: "workflow compilation failed",
        source:  Box::new(WorkflowError::engine_with_source(
            "workflow create task failed",
            source,
        )),
    })?
    .map_err(classify_workflow_error)?;

    Ok(PersistenceReadyRun {
        materialized,
        entrypoint,
        metadata,
    })
}

/// Purely assemble the complete workflow persistence input.
pub(crate) fn assemble_run(ready: PersistenceReadyRun) -> RunCompilerOutput {
    let PersistenceReadyRun {
        materialized,
        entrypoint,
        metadata,
    } = ready;
    let RunMetadata {
        run_id,
        title,
        parent_id,
        git,
        storage_root,
        workflow_slug,
        provenance,
        web_url,
        submitted_manifest_bytes,
        automation,
    } = metadata;
    let persistence_input = operations::assemble_create_run_persistence_input(
        materialized,
        CreateRunPersistenceMetadata {
            run_id: run_id.unwrap_or_default(),
            storage_root,
            workflow_slug,
            submitted_manifest_bytes,
            title,
            automation,
            git,
            fork_source_ref: None,
            parent_id,
            provenance,
            web_url,
        },
    );

    RunCompilerOutput {
        persistence_input,
        entrypoint,
    }
}

/// Compile a raw run all the way to a complete persistence input.
#[cfg(test)]
pub(crate) async fn compile_run(
    input: RawRunCompilerInput,
    catalog: Arc<Catalog>,
) -> Result<RunCompilerOutput> {
    let prepared = prepare_run(input)?;
    let compiled = compile_graph(prepared, Arc::clone(&catalog)).await?;
    let materialized = materialize_run(compiled, catalog).await?;
    Ok(assemble_run(materialized))
}

fn parse_settings_layer(
    source: &str,
    config_path: &ManifestPath,
    files: &HashMap<ManifestPath, String>,
    settings_source: SettingsSource,
    kind: &'static str,
) -> Result<SettingsLayer> {
    let mut layer =
        source
            .parse::<SettingsLayer>()
            .map_err(|source| RunCompilerError::InvalidSettings {
                source: Box::new(InvalidSettingsError::Parse {
                    kind,
                    path: config_path.clone(),
                    source: Box::new(source),
                }),
            })?;
    parse::validate_settings_source(&layer, settings_source).map_err(|source| {
        RunCompilerError::InvalidSettings {
            source: Box::new(InvalidSettingsError::Parse {
                kind,
                path: config_path.clone(),
                source: Box::new(source),
            }),
        }
    })?;
    resolve_dockerfiles(&mut layer, config_path, files)?;
    Ok(layer)
}

fn resolve_dockerfiles(
    layer: &mut SettingsLayer,
    config_path: &ManifestPath,
    files: &HashMap<ManifestPath, String>,
) -> Result<()> {
    for environment in layer.environments.values_mut() {
        if let Some(image) = environment.image.as_mut() {
            resolve_dockerfile(image, config_path, files)?;
        }
    }
    if let Some(image) = layer
        .run
        .as_mut()
        .and_then(|run| run.environment.as_mut())
        .and_then(|environment| environment.image.as_mut())
    {
        resolve_dockerfile(image, config_path, files)?;
    }
    Ok(())
}

fn resolve_dockerfile(
    image: &mut EnvironmentImageLayer,
    config_path: &ManifestPath,
    files: &HashMap<ManifestPath, String>,
) -> Result<()> {
    let Some(EnvironmentDockerfileLayer::Path { path }) = image.dockerfile.as_ref() else {
        return Ok(());
    };
    let reference = path.clone();
    let dockerfile_path = ManifestPath::from_reference(config_path.parent_or_dot(), &reference)
        .ok_or_else(|| RunCompilerError::InvalidSource {
            source: InvalidSourceError::UnsupportedDockerfileReference {
                config_path: config_path.clone(),
                reference:   reference.clone(),
            },
        })?;
    let content =
        files
            .get(&dockerfile_path)
            .cloned()
            .ok_or_else(|| RunCompilerError::InvalidSource {
                source: InvalidSourceError::MissingDockerfile {
                    config_path:     config_path.clone(),
                    dockerfile_path: dockerfile_path.clone(),
                },
            })?;
    image.dockerfile = Some(EnvironmentDockerfileLayer::Inline(content));
    Ok(())
}

fn substitute_variables(
    variables: &HashMap<String, String>,
    settings: &mut WorkflowSettings,
) -> Result<()> {
    settings
        .run
        .substitute_variables(|name| variables.get(name).cloned())
        .map_err(|source| RunCompilerError::VariableInterpolation {
            source: VariableInterpolationError::Interpolation(source),
        })?;
    for (index, pattern) in settings.run.artifacts.include.iter().enumerate() {
        WorkspaceGlob::try_new(pattern).map_err(|source| {
            RunCompilerError::VariableInterpolation {
                source: VariableInterpolationError::ArtifactGlob { index, source },
            }
        })?;
    }
    Ok(())
}

fn classify_workflow_error(error: WorkflowError) -> RunCompilerError {
    match error {
        WorkflowError::ModelSelection(source) => RunCompilerError::ModelSelection { source },
        WorkflowError::ModelReference(source) => RunCompilerError::ModelReference { source },
        source @ (WorkflowError::Parse(_) | WorkflowError::ValidationFailed { .. }) => {
            RunCompilerError::ValidationOrParse { source }
        }
        source => RunCompilerError::Internal {
            context: "workflow compilation failed",
            source:  Box::new(source),
        },
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::error::Error as _;
    use std::sync::Arc;

    use fabro_config::EnvironmentDockerfileLayer;
    use fabro_graphviz::graph::AttrValue;
    use fabro_model::Catalog;
    use fabro_types::settings::interp::ResolveCtx;
    use fabro_types::settings::run::RunGoal;
    use fabro_types::{AutomationRef, Principal, RunProvenance, SystemActorKind};
    use fabro_workflow::workflow_bundle::ParsedWorkflowConfig;

    use super::*;

    const DOT: &str = r#"digraph Test {
        graph [goal="Graph goal"]
        start [shape=Mdiamond]
        work [prompt="Ship {{ inputs.target }} for {{ vars.owner }}", model="gpt-5.4"]
        exit [shape=Msquare]
        start -> work -> exit
    }"#;

    fn manifest_path(value: &str) -> ManifestPath {
        ManifestPath::from_wire(value).expect("fixture manifest path should be valid")
    }

    fn provenance() -> RunProvenance {
        RunProvenance {
            server:  None,
            client:  None,
            subject: Principal::System {
                system_kind: SystemActorKind::Engine,
            },
        }
    }

    fn workflow(
        entrypoint: &ManifestPath,
        workflow_toml: Option<&str>,
        files: HashMap<ManifestPath, String>,
    ) -> BundledWorkflow {
        BundledWorkflow {
            path: entrypoint.clone(),
            source: DOT.to_string(),
            config: workflow_toml.map(|source| ParsedWorkflowConfig {
                path:   manifest_path("flows/workflow.toml"),
                source: source.to_string(),
            }),
            files,
        }
    }

    fn raw_input(
        workflow_toml: Option<&str>,
        files: HashMap<ManifestPath, String>,
    ) -> RawRunCompilerInput {
        let entrypoint = manifest_path("flows/workflow.fabro");
        let workflow = workflow(&entrypoint, workflow_toml, files);
        RawRunCompilerInput {
            workflow_bundle: WorkflowBundle::new(HashMap::from([(entrypoint.clone(), workflow)])),
            entrypoint,
            cwd: PathBuf::from("/workspace"),
            server_run_defaults: RunLayer::default(),
            server_environment_defaults: fabro_environment::seeded_catalog_layer(),
            server_mcp_catalog: HashMap::new(),
            project_settings: Vec::new(),
            user_toml: Vec::new(),
            run_overrides: None,
            cli_overrides: None,
            input_overrides: HashMap::new(),
            inline_goal_override: None,
            vars: HashMap::new(),
            run_id: Some(RunId::new()),
            title: None,
            parent_id: None,
            git: None,
            storage_root: PathBuf::from("/tmp/fabro-storage"),
            configured_providers: Catalog::builtin().all_provider_ids().into_iter().collect(),
            workflow_slug: None,
            provenance: provenance(),
            web_url: None,
            submitted_manifest_bytes: None,
            automation: None,
        }
    }

    #[test]
    fn stage_one_rejects_missing_entrypoint() {
        let mut input = raw_input(None, HashMap::new());
        input.entrypoint = manifest_path("flows/missing.fabro");

        let Err(error) = normalize_source(input) else {
            panic!("missing entrypoint should fail");
        };

        assert!(matches!(error, RunCompilerError::InvalidSource {
            source: InvalidSourceError::MissingEntrypoint { .. },
        }));
    }

    #[test]
    fn unresolved_run_id_is_allocated_only_after_variables_are_applied() {
        let mut input = raw_input(None, HashMap::new());
        input.run_id = None;
        let normalized = normalize_source(input).expect("source should normalize");
        let layered = layer_settings(normalized).expect("settings should layer");
        let prepared = apply_run_variables(layered).expect("variables should apply");

        let (prepared, run_id) = prepared.resolve_run_id();

        assert_eq!(prepared.metadata.run_id, Some(run_id));
    }

    #[test]
    fn stage_one_rejects_missing_dockerfile_and_preserves_source_chain() {
        let workflow_toml = r#"
_version = 1

[run.environment.image]
dockerfile = { path = "Dockerfile" }
"#;

        let Err(error) = normalize_source(raw_input(Some(workflow_toml), HashMap::new())) else {
            panic!("missing dockerfile should fail");
        };

        assert!(matches!(error, RunCompilerError::InvalidSource {
            source: InvalidSourceError::MissingDockerfile { .. },
        }));
        let source = error
            .source()
            .expect("top-level error should retain source");
        assert!(source.to_string().contains("Dockerfile"));
    }

    #[test]
    fn stage_one_resolves_bundled_dockerfile() {
        let workflow_toml = r#"
_version = 1

[run.environment.image]
dockerfile = { path = "Dockerfile" }
"#;
        let normalized = normalize_source(raw_input(
            Some(workflow_toml),
            HashMap::from([(
                manifest_path("flows/Dockerfile"),
                "FROM ubuntu:24.04\n".to_string(),
            )]),
        ))
        .expect("bundled dockerfile should resolve");
        let dockerfile = normalized
            .workflow_layer
            .as_ref()
            .and_then(|layer| layer.run.as_ref())
            .and_then(|run| run.environment.as_ref())
            .and_then(|environment| environment.image.as_ref())
            .and_then(|image| image.dockerfile.as_ref());

        assert_eq!(
            dockerfile,
            Some(&EnvironmentDockerfileLayer::Inline(
                "FROM ubuntu:24.04\n".to_string()
            ))
        );
    }

    #[test]
    fn settings_apply_precedence_vars_inputs_and_safe_artifact_globs() {
        let workflow_toml = r#"
_version = 1

[run.metadata]
layer = "workflow"
owner = "{{ vars.owner }}"

[run.inputs]
target = "workflow"

[run.artifacts]
include = ["reports/{{ vars.owner }}/*.json"]
"#;
        let mut input = raw_input(Some(workflow_toml), HashMap::new());
        input.project_settings.push(ProjectSettingsSource {
            path: manifest_path(".fabro/project.toml"),
            toml: r#"
_version = 1

[run.metadata]
layer = "project"
"#
            .to_string(),
        });
        input.user_toml = vec![
            r#"
_version = 1

[run.metadata]
layer = "user"
"#
            .to_string(),
        ];
        input.run_overrides = Some(
            toml::from_str::<SettingsLayer>(
                r#"
_version = 1

[run.metadata]
layer = "args"
owner = "{{ vars.owner }}"
"#,
            )
            .expect("args settings should parse")
            .run
            .expect("args run layer should exist"),
        );
        input.input_overrides.insert(
            "target".to_string(),
            toml::Value::String("override".to_string()),
        );
        input.inline_goal_override = Some("Ship {{ vars.owner }}".to_string());
        input
            .vars
            .insert("owner".to_string(), "payments".to_string());

        let prepared = prepare_run(input).expect("settings should prepare");
        let settings = prepared.settings();

        assert_eq!(
            settings.run.metadata.get("layer").map(String::as_str),
            Some("args")
        );
        assert_eq!(
            settings.run.metadata.get("owner").map(String::as_str),
            Some("payments")
        );
        assert_eq!(
            settings.run.inputs.get("target"),
            Some(&toml::Value::String("override".to_string()))
        );
        assert_eq!(settings.run.artifacts.include, vec![
            "reports/payments/*.json"
        ]);
        let Some(RunGoal::Inline(goal)) = settings.run.goal.as_ref() else {
            panic!("inline goal override should win");
        };
        assert_eq!(
            goal.resolve_with(&mut ResolveCtx::default()).unwrap(),
            "Ship payments"
        );
    }

    #[test]
    fn settings_reject_artifact_glob_made_unsafe_by_variable() {
        let workflow_toml = r#"
_version = 1

[run.artifacts]
include = ["reports/{{ vars.path }}/*.json"]
"#;
        let mut input = raw_input(Some(workflow_toml), HashMap::new());
        input
            .vars
            .insert("path".to_string(), "../secrets".to_string());

        let Err(error) = prepare_run(input) else {
            panic!("unsafe artifact glob should fail");
        };

        assert!(matches!(error, RunCompilerError::VariableInterpolation {
            source: VariableInterpolationError::ArtifactGlob { .. },
        }));
    }

    #[tokio::test]
    async fn graph_vars_are_hard_errors_and_successfully_render_when_present() {
        let catalog = Arc::new(Catalog::from_builtin().unwrap());
        let missing = prepare_run(raw_input(None, HashMap::new()))
            .expect("settings preparation should not compile graph vars");
        let Err(error) = compile_graph(missing, Arc::clone(&catalog)).await else {
            panic!("missing graph variable should be a hard error");
        };
        assert!(matches!(error, RunCompilerError::ValidationOrParse {
            source: WorkflowError::ValidationFailed { .. },
        }));

        let mut input = raw_input(None, HashMap::new());
        input
            .vars
            .insert("owner".to_string(), "payments".to_string());
        input.input_overrides.insert(
            "target".to_string(),
            toml::Value::String("checkout".to_string()),
        );
        let compiled = compile_graph(
            prepare_run(input).expect("settings should prepare"),
            catalog,
        )
        .await
        .expect("graph variables should render");
        let work = &compiled.compiled().validated().graph().nodes["work"];

        assert_eq!(
            work.attrs.get("prompt").and_then(AttrValue::as_str),
            Some("Ship checkout for payments")
        );
        assert_eq!(
            work.attrs.get("provider").and_then(AttrValue::as_str),
            Some("openai")
        );
    }

    #[tokio::test]
    async fn assembly_retains_entrypoint_and_run_metadata() {
        let run_id = RunId::new();
        let parent_id = RunId::new();
        let automation = AutomationRef {
            id:         "nightly".to_string(),
            name:       Some("Nightly".to_string()),
            trigger_id: Some("schedule".to_string()),
        };
        let submitted = b"submitted manifest".to_vec();
        let mut input = raw_input(None, HashMap::new());
        input.run_id = Some(run_id);
        input.parent_id = Some(parent_id);
        input.title = Some("Compiler boundary".to_string());
        input.workflow_slug = Some("compiler-boundary".to_string());
        input.web_url = Some(format!("https://fabro.test/runs/{run_id}"));
        input.submitted_manifest_bytes = Some(submitted.clone());
        input.automation = Some(automation.clone());
        input
            .vars
            .insert("owner".to_string(), "payments".to_string());
        input.input_overrides.insert(
            "target".to_string(),
            toml::Value::String("checkout".to_string()),
        );
        let expected_entrypoint = input.entrypoint.clone();

        let output = compile_run(input, Arc::new(Catalog::from_builtin().unwrap()))
            .await
            .expect("run should compile");
        let persistence = output.persistence_input();

        assert_eq!(output.entrypoint(), &expected_entrypoint);
        assert_eq!(persistence.run_id(), run_id);
        assert_eq!(persistence.workflow_slug(), Some("compiler-boundary"));
        assert_eq!(
            persistence.submitted_manifest_bytes(),
            Some(submitted.as_slice())
        );
        assert_eq!(persistence.automation(), Some(&automation));
        assert_eq!(
            persistence
                .definition()
                .map(|definition| &definition.workflow_path),
            Some(&expected_entrypoint)
        );
        assert_eq!(
            persistence.materialized().settings().run.goal.as_ref(),
            Some(&RunGoal::Inline(InterpString::parse("Graph goal")))
        );
    }
}
