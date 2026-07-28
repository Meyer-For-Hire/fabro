//! The profile for GPT-5.6 models (Sol, Terra, Luna).
//!
//! These models were trained against Codex, whose core tool set is far narrower
//! than what fabro offers the other OpenAI models: a shell, `apply_patch`, and
//! `update_plan`, plus web search when configured. Codex has no dedicated
//! file-read, file-write, grep, glob, or fetch tool -- reading and searching
//! local files go through the shell, and writes go through `apply_patch`.
//! OpenAI-compatible gateways cannot carry that freeform tool, so those routes
//! receive fabro's JSON-schema `edit_file` fallback instead.
//!
//! One deliberate difference from Codex: Codex drives 5.6 in *code mode*,
//! exposing a single `exec` tool that takes JavaScript and reaching every other
//! tool through a `tools` object inside a V8 isolate. Fabro calls tools
//! directly, so this profile matches Codex's tool *contract* -- names,
//! parameters, and guidance -- without that indirection.

use std::sync::Arc;

use fabro_llm::types::ToolDefinition;
use fabro_model::{AgentProfileKind, Catalog, ProviderId};
use serde_json::Value;

use super::EnvContext;
use crate::agent_profile::AgentProfile;
use crate::config::NativeToolOptions;
use crate::native_tool::{NativeTool, ToolVocabulary};
use crate::profiles::{
    self, BaseProfile, EmbeddedPrompt, FileEditToolKind, ProfileDeps, impl_base_profile_accessors,
};
use crate::sandbox::Sandbox;
use crate::skills::Skill;
use crate::todo_runtime::TodoRuntime;
use crate::todo_tools::make_update_plan_tool;
use crate::tool_registry::{RegisteredTool, ToolRegistry, ToolSource};
use crate::{apply_patch, tools};

const CORE_PROMPT: &str = include_str!("prompts/gpt56.md.j2");

pub struct Gpt56Profile {
    base:                     BaseProfile,
    /// Retained so the shell tool's description can be rebuilt when the file
    /// editor is swapped out on a codec that cannot carry a freeform tool.
    shell_default_timeout_ms: u64,
    shell_max_timeout_ms:     u64,
}

impl Gpt56Profile {
    #[must_use]
    pub fn new(model: impl Into<String>) -> Self {
        let deps = ProfileDeps::standalone(NativeToolOptions::for_profile(AgentProfileKind::Gpt56));
        Self::with_native_tools(model, &deps)
    }

    /// `deps.summarizer` is ignored: this profile exposes no `web_fetch`.
    pub(crate) fn with_native_tools(model: impl Into<String>, deps: &ProfileDeps) -> Self {
        let options = &deps.options;
        // The registry carries the vocabulary, so tools registered later --
        // subagent tools, skills -- are named consistently too.
        let mut registry = ToolRegistry::with_vocabulary(ToolVocabulary::Codex);

        registry.register(make_shell_command_tool(options));
        registry.register(apply_patch::make_apply_patch_tool());
        let todo_runtime = Arc::new(TodoRuntime::new());
        registry.register(make_update_plan_tool(todo_runtime));
        // Codex gives 5.6 a search tool (its namespaced `web.run`), so search
        // is not an untrained affordance the way fabro's `web_fetch` would be.
        tools::register_web_search_tool(&mut registry, options);

        Self {
            base:                     BaseProfile {
                profile_kind: AgentProfileKind::Gpt56,
                provider_id: ProviderId::openai(),
                model: model.into(),
                catalog: None,
                registry,
            },
            shell_default_timeout_ms: options.default_command_timeout_ms,
            shell_max_timeout_ms:     options.max_command_timeout_ms,
        }
    }

    /// Configure the provider and catalog together so the route's codec
    /// determines which file editor is registered.
    ///
    /// GPT-5.6 is served both directly by OpenAI and through gateways such as
    /// OpenRouter, so the provider is not fixed by the profile.
    #[must_use]
    pub fn with_route(mut self, provider_id: ProviderId, catalog: Arc<Catalog>) -> Self {
        self.base.set_route(provider_id, catalog);
        if let Some(file_edit_tool) = self.base.configure_file_edit_tool() {
            // The shell tool points at the file editor by name, so its
            // description has to move too or it names a tool the model was
            // never given.
            self.base.registry.redescribe(
                NativeTool::Shell,
                shell_command_description(
                    self.shell_default_timeout_ms,
                    self.shell_max_timeout_ms,
                    file_edit_tool,
                ),
            );
        }
        self
    }
}

/// Codex's `shell_command`: a shell script plus an explicit `workdir`.
///
/// Fabro's own `shell` tool has no `workdir` and its description steers the
/// model toward the dedicated read and search tools. Neither fits here: 5.6 has
/// no dedicated tools to steer toward, and Codex tells it to set `workdir`
/// rather than `cd`.
fn shell_command_description(
    default_timeout_ms: u64,
    max_timeout_ms: u64,
    file_edit_tool: FileEditToolKind,
) -> String {
    let file_edit_tool: &'static str = file_edit_tool.into();
    format!(
        "Runs a shell command and returns its output.
- Always set the `workdir` param rather than using `cd`.
- Reading and searching files goes through this tool: prefer `rg` and \
`rg --files`, which are much faster than alternatives like `grep` and `find`.
- Use `{file_edit_tool}` to edit files, not `cat`, heredocs, or other shell write tricks.
- `timeout_ms` defaults to {default_timeout_ms} ms and is capped at {max_timeout_ms} ms. A command \
that timed out once will time out again, so raise the timeout rather than retrying."
    )
}

fn make_shell_command_tool(options: &NativeToolOptions) -> RegisteredTool {
    let default_timeout_ms = options.default_command_timeout_ms;
    let max_timeout_ms = options.max_command_timeout_ms;
    let description = shell_command_description(
        default_timeout_ms,
        max_timeout_ms,
        FileEditToolKind::ApplyPatch,
    );

    RegisteredTool {
        definition: ToolDefinition {
            // Supply the canonical identity; registry insertion rewrites the
            // stored and wire name to `shell_command`.
            name: NativeTool::Shell.canonical_name().to_string(),
            description,
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "Bash source to evaluate, run by a non-login Bash shell."
                    },
                    "workdir": {
                        "type": "string",
                        "description": "Working directory for the command. Defaults to the turn cwd."
                    },
                    "timeout_ms": {
                        "type": "integer",
                        "description": format!(
                            "Maximum command runtime. Defaults to {default_timeout_ms} ms."
                        )
                    }
                },
                "required": ["command"]
            }),
        },
        executor:   Arc::new(move |args, ctx| {
            Box::pin(async move {
                let command = tools::required_str(&args, "command")?;
                let workdir = args.get("workdir").and_then(Value::as_str);
                let timeout_ms = args
                    .get("timeout_ms")
                    .and_then(Value::as_u64)
                    .unwrap_or(default_timeout_ms)
                    .min(max_timeout_ms);

                tools::run_shell_command(&ctx, command, timeout_ms, workdir).await
            })
        }),
        source:     ToolSource::Native,
    }
}

impl AgentProfile for Gpt56Profile {
    impl_base_profile_accessors!();

    fn build_system_prompt(
        &self,
        env: &dyn Sandbox,
        env_context: &EnvContext,
        memory: &[String],
        user_instructions: Option<&str>,
        skills: &[Skill],
    ) -> String {
        let has_web_search = self
            .base
            .registry
            .get(tools::WEB_SEARCH_TOOL_NAME)
            .is_some();
        let file_edit_tool: &'static str = self
            .base
            .file_edit_tool()
            .expect("GPT-5.6 profile should register exactly one file-editing tool")
            .into();
        let template = EmbeddedPrompt::new("gpt56.md.j2", CORE_PROMPT)
            .with_vocabulary(ToolVocabulary::Codex)
            .with_string("provider_name", self.base.provider_display_name())
            .with_string("file_edit_tool", file_edit_tool)
            .with_bool("has_web_search", has_web_search);

        profiles::assemble_system_prompt(
            template,
            env,
            env_context,
            memory,
            user_instructions,
            skills,
        )
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use fabro_model::catalog::LlmCatalogSettings;

    use super::*;
    use crate::subagent::{SessionFactory, SubAgentSupervisor};
    use crate::test_support::MockSandbox;

    fn test_catalog() -> Arc<Catalog> {
        Arc::new(Catalog::from_builtin().unwrap())
    }

    /// OpenRouter ships disabled in the built-in catalog.
    fn catalog_with_openrouter() -> Arc<Catalog> {
        let overrides: LlmCatalogSettings =
            toml::from_str("[providers.openrouter]\nenabled = true\n").unwrap();
        Arc::new(Catalog::from_builtin_with_overrides(&overrides).unwrap())
    }

    fn prompt(profile: &Gpt56Profile) -> String {
        let env = MockSandbox::linux();
        profile.build_system_prompt(&env, &EnvContext::default(), &[], None, &[])
    }

    #[test]
    fn gpt56_profile_identity() {
        let profile = Gpt56Profile::new("gpt-5.6-sol");
        assert_eq!(profile.profile_kind(), AgentProfileKind::Gpt56);
        assert_eq!(profile.provider_id(), ProviderId::openai());
        assert_eq!(profile.model(), "gpt-5.6-sol");
    }

    /// The whole point of the profile: 5.6 sees Codex's tools and nothing else.
    #[test]
    fn gpt56_registers_only_codex_tools() {
        let profile = Gpt56Profile::new("gpt-5.6-sol");
        let mut names = profile.tool_registry().names();
        names.sort();
        assert_eq!(names, vec!["apply_patch", "shell_command", "update_plan"]);
    }

    #[test]
    fn gpt56_omits_the_tools_codex_does_not_have() {
        let profile = Gpt56Profile::new("gpt-5.6-terra");
        let names = profile.tool_registry().names();
        for absent in [
            "read_file",
            "write_file",
            "edit_file",
            "grep",
            "glob",
            "web_fetch",
            "shell",
        ] {
            assert!(
                !names.contains(&absent.to_string()),
                "gpt56 profile should not register {absent}"
            );
        }
    }

    #[test]
    fn shell_command_accepts_a_workdir() {
        let profile = Gpt56Profile::new("gpt-5.6-sol");
        let shell = profile.tool_registry().get("shell_command").unwrap();
        assert_eq!(shell.definition.parameters["type"], "object");
        assert!(shell.definition.parameters["properties"]["workdir"].is_object());
        assert_eq!(
            shell.definition.parameters["required"],
            serde_json::json!(["command"])
        );
        assert_eq!(
            shell.definition.parameters["properties"]["command"]["description"],
            "Bash source to evaluate, run by a non-login Bash shell."
        );
    }

    /// The `openai_compatible` codec rejects custom tool definitions outright,
    /// so a freeform `apply_patch` on that route fails every request. 5.6 is
    /// served through OpenRouter, which uses exactly that codec.
    #[test]
    fn gateway_routes_swap_apply_patch_for_a_json_schema_editor() {
        let profile = Gpt56Profile::new("gpt-5.6-sol")
            .with_route(ProviderId::new("openrouter"), catalog_with_openrouter());

        let names = profile.tool_registry().names();
        assert!(names.contains(&"edit_file".to_string()));
        assert!(!names.contains(&"apply_patch".to_string()));

        for definition in profile.tool_registry().definitions() {
            assert!(
                !definition.is_custom(),
                "tool '{}' must not be a custom definition on an openai_compatible route",
                definition.name
            );
            assert_eq!(definition.parameters["type"], "object");
        }
    }

    /// The shell tool names the file editor, so it has to follow the swap or
    /// it points 5.6 at a tool it was never given.
    #[test]
    fn shell_description_names_the_editor_actually_registered() {
        let direct =
            Gpt56Profile::new("gpt-5.6-sol").with_route(ProviderId::openai(), test_catalog());
        let shell = direct.tool_registry().get("shell_command").unwrap();
        assert!(shell.definition.description.contains("`apply_patch`"));
        assert!(!shell.definition.description.contains("`edit_file`"));

        let gateway = Gpt56Profile::new("gpt-5.6-sol")
            .with_route(ProviderId::new("openrouter"), catalog_with_openrouter());
        let shell = gateway.tool_registry().get("shell_command").unwrap();
        assert!(shell.definition.description.contains("`edit_file`"));
        assert!(!shell.definition.description.contains("`apply_patch`"));
    }

    #[test]
    fn prompt_describes_the_editor_actually_registered() {
        let gateway = Gpt56Profile::new("gpt-5.6-sol")
            .with_route(ProviderId::new("openrouter"), catalog_with_openrouter());
        let rendered = prompt(&gateway);
        assert!(rendered.contains("Use `edit_file` for local file edits"));
        assert!(!rendered.contains("apply_patch"));
        assert!(!rendered.contains("*** Begin Patch"));

        let direct =
            Gpt56Profile::new("gpt-5.6-sol").with_route(ProviderId::openai(), test_catalog());
        let rendered = prompt(&direct);
        assert!(rendered.contains("Use `apply_patch` for local file edits"));
        assert!(rendered.contains("*** Begin Patch"));
        assert!(!rendered.contains("edit_file"));
    }

    #[test]
    fn apply_patch_stays_a_freeform_grammar_tool() {
        let profile = Gpt56Profile::new("gpt-5.6-luna");
        let apply_patch = profile.tool_registry().get("apply_patch").unwrap();
        assert!(apply_patch.definition.is_custom());
    }

    #[test]
    fn web_search_is_registered_only_when_a_key_is_configured() {
        let profile = Gpt56Profile::new("gpt-5.6-sol");
        assert!(profile.tool_registry().get("web_search").is_none());
        assert!(!prompt(&profile).contains("web_search"));

        let mut options = NativeToolOptions::for_profile(AgentProfileKind::Gpt56);
        options.secrets.brave_search_api_key = Some("configured-key".to_string());
        let deps = ProfileDeps::standalone(options);
        let searching = Gpt56Profile::with_native_tools("gpt-5.6-sol", &deps);
        assert!(searching.tool_registry().get("web_search").is_some());
        assert!(prompt(&searching).contains("web_search"));
    }

    #[test]
    fn gpt56_subagent_tools_registered() {
        let mut profile = Gpt56Profile::new("gpt-5.6-sol");
        assert_eq!(profile.tool_registry().names().len(), 3);

        let supervisor = SubAgentSupervisor::new(3);
        let factory: SessionFactory = Arc::new(|| panic!("should not be called in test"));
        profile.register_subagent_tools(supervisor, factory, 0);
        assert_eq!(profile.tool_registry().names().len(), 7);
    }

    #[test]
    fn prompt_names_the_shell_tool_as_codex_does() {
        let rendered = prompt(&Gpt56Profile::new("gpt-5.6-sol"));
        assert!(rendered.contains("shell_command"));
        assert!(rendered.contains("apply_patch"));
        // The tools 5.6 does not have must not be named as if it did. `grep`,
        // `find`, and `glob` are excluded from this list on purpose: the prompt
        // names them as shell CLIs and shell concepts, which is what Codex
        // does, not as tools fabro registers.
        for absent in ["read_file", "write_file", "edit_file", "web_fetch"] {
            assert!(
                !rendered.contains(absent),
                "prompt should not mention {absent}"
            );
        }
    }

    #[test]
    fn prompt_contains_env_context_and_memory_and_user_instructions() {
        let profile = Gpt56Profile::new("gpt-5.6-sol");
        let env = MockSandbox::linux();
        let docs = vec!["# Project README".to_string()];
        let rendered = profile.build_system_prompt(
            &env,
            &EnvContext::default(),
            &docs,
            Some("Always write tests first"),
            &[],
        );
        assert!(rendered.contains("<environment>"));
        assert!(rendered.contains("linux"));
        assert!(rendered.contains("# Project README"));
        assert!(rendered.contains("# User Instructions"));
        assert!(rendered.contains("Always write tests first"));
    }

    #[test]
    fn provider_prompt_uses_catalog_display_name() {
        let direct =
            Gpt56Profile::new("gpt-5.6-sol").with_route(ProviderId::openai(), test_catalog());
        assert!(prompt(&direct).contains("powered by OpenAI"));

        let gateway = Gpt56Profile::new("gpt-5.6-sol")
            .with_route(ProviderId::new("openrouter"), catalog_with_openrouter());
        assert!(prompt(&gateway).contains("powered by OpenRouter"));
    }

    /// The three 5.6 models must resolve to this profile wherever they are
    /// served, and the other models on those providers must not.
    #[test]
    fn only_the_5_6_models_select_the_gpt56_profile() {
        for (catalog, provider) in [
            (test_catalog(), "openai"),
            (catalog_with_openrouter(), "openrouter"),
        ] {
            let provider_id = ProviderId::new(provider);
            for model in ["gpt-5.6-sol", "gpt-5.6-terra", "gpt-5.6-luna"] {
                assert_eq!(
                    catalog.effective_agent_profile(&provider_id, Some(model)),
                    Some(AgentProfileKind::Gpt56),
                    "{provider}/{model} should use the gpt56 profile"
                );
            }
            for model in ["gpt-5.5", "gpt-5.4"] {
                assert_eq!(
                    catalog.effective_agent_profile(&provider_id, Some(model)),
                    Some(AgentProfileKind::OpenAi),
                    "{provider}/{model} should keep the openai profile"
                );
            }
        }
    }

    #[test]
    fn catalog_reports_the_5_6_context_window() {
        let profile =
            Gpt56Profile::new("gpt-5.6-sol").with_route(ProviderId::openai(), test_catalog());
        assert_eq!(profile.context_window_size(), 272_000);
    }
}
