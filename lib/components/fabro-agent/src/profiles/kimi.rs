use std::sync::Arc;

use fabro_model::{AgentProfileKind, Catalog, ProviderId};

use super::EnvContext;
use crate::agent_profile::AgentProfile;
use crate::config::NativeToolOptions;
use crate::native_tool::{NativeTool, ToolVocabulary};
use crate::profiles::{self, BaseProfile, EmbeddedPrompt};
use crate::sandbox::Sandbox;
use crate::skills::Skill;
use crate::todo_runtime::TodoRuntime;
use crate::todo_tools::make_todo_list_tool;
use crate::tool_registry::ToolRegistry;
use crate::tools::{WebFetchSummarizer, make_edit_file_tool, register_core_tools};

const CORE_PROMPT: &str = include_str!("prompts/kimi.md.j2");

/// Kimi models repeatedly fail the workspace's read-before-write guard: across
/// two observed K3 implementation stages, 32 of 35 tool failures were writes to
/// files the model had not read, or `old_string` values reconstructed from
/// memory. Kimi Code's own tool descriptions drill this rule directly, so the
/// Kimi profile restates it where the model is most likely to act on it — in
/// the description of the tool being called — rather than relying only on the
/// system prompt.
const EDIT_FILE_DESCRIPTION: &str = "Edit a file by replacing an exact string. \
Read the file with Read before EVERY edit — this workspace refuses writes to files that \
have not been read, and the call will fail. Take old_string verbatim from the Read output; \
never reconstruct it from memory or from an earlier version of the file. old_string must be an \
exact match and unique unless replace_all is true. If the edit fails with 'old_string not found', \
re-read the file and take the exact text from the fresh output rather than guessing again. \
Preserve existing indentation.";

const WRITE_FILE_DESCRIPTION: &str = "Create a new file, or completely replace an existing one. \
Read an existing file with Read before writing to it — this workspace refuses writes to \
files that have not been read, and the call will fail. Prefer Edit for any incremental \
change: Write replaces the entire file, so using it to make a small edit discards \
everything you did not restate.";

pub struct KimiProfile {
    base: BaseProfile,
}

impl KimiProfile {
    #[must_use]
    pub fn new(model: impl Into<String>) -> Self {
        let options = NativeToolOptions::for_profile(AgentProfileKind::Kimi);
        Self::with_native_tools(model, &options, None)
    }

    pub(crate) fn with_native_tools(
        model: impl Into<String>,
        options: &NativeToolOptions,
        summarizer: Option<WebFetchSummarizer>,
    ) -> Self {
        // The registry carries the vocabulary, so tools registered later
        // (subagent tools, skills) are renamed too.
        let mut registry = ToolRegistry::with_vocabulary(ToolVocabulary::KimiCode);

        register_core_tools(&mut registry, options, summarizer);
        registry.register(make_edit_file_tool());
        registry.redescribe(NativeTool::EditFile, EDIT_FILE_DESCRIPTION);
        registry.redescribe(NativeTool::WriteFile, WRITE_FILE_DESCRIPTION);

        // Kimi Code drives todos with one replace-whole-list call. The
        // Anthropic task tools model the opposite interaction -- incremental
        // mutation against tracked ids -- so they are the wrong surface here
        // even though both persist through the same runtime.
        let todo_runtime = Arc::new(TodoRuntime::new());
        registry.register(make_todo_list_tool(todo_runtime));

        Self {
            base: BaseProfile {
                profile_kind: AgentProfileKind::Kimi,
                provider_id: ProviderId::new("kimi"),
                model: model.into(),
                catalog: None,
                registry,
            },
        }
    }

    /// Override the provider ID while retaining the adapter/profile behavior.
    ///
    /// Kimi models are served both directly by Moonshot and through gateways
    /// such as OpenRouter, so the provider is not fixed by the profile.
    #[must_use]
    pub fn with_provider_id(mut self, provider_id: ProviderId) -> Self {
        self.base.provider_id = provider_id;
        self
    }

    #[must_use]
    pub fn with_catalog(mut self, catalog: Arc<Catalog>) -> Self {
        self.base.catalog = Some(catalog);
        self
    }
}

impl AgentProfile for KimiProfile {
    fn profile_kind(&self) -> AgentProfileKind {
        self.base.profile_kind
    }

    fn provider_id(&self) -> ProviderId {
        self.base.provider_id.clone()
    }

    fn model(&self) -> &str {
        &self.base.model
    }

    fn catalog(&self) -> Option<&Catalog> {
        self.base.catalog.as_deref()
    }

    fn tool_registry(&self) -> &ToolRegistry {
        &self.base.registry
    }

    fn tool_registry_mut(&mut self) -> &mut ToolRegistry {
        &mut self.base.registry
    }

    fn build_system_prompt(
        &self,
        env: &dyn Sandbox,
        env_context: &EnvContext,
        memory: &[String],
        user_instructions: Option<&str>,
        skills: &[Skill],
    ) -> String {
        let template = EmbeddedPrompt::new("kimi.md.j2", CORE_PROMPT)
            .with_vocabulary(self.base.registry.vocabulary());

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
    use fabro_model::catalog::LlmCatalogSettings;
    use fabro_types::AgentToolCategory;

    use super::*;
    use crate::skills::make_use_skill_tool;
    use crate::subagent::{SessionFactory, SubAgentSupervisor};
    use crate::test_support::MockSandbox;
    use crate::tool_permissions::{known_tool_category, tool_category};

    fn catalog() -> Arc<Catalog> {
        Arc::new(Catalog::from_builtin().unwrap())
    }

    /// OpenRouter ships disabled, so an operator opts in before its models are
    /// selectable. Enable it the way they would, to observe gateway routing.
    fn catalog_with_openrouter() -> Arc<Catalog> {
        let overrides: LlmCatalogSettings =
            toml::from_str("[providers.openrouter]\nenabled = true\n").unwrap();
        Arc::new(Catalog::from_builtin_with_overrides(&overrides).unwrap())
    }

    /// Kimi models must resolve to the Kimi profile whether they are reached
    /// directly at Moonshot or through a gateway such as OpenRouter.
    #[test]
    fn kimi_models_select_the_kimi_profile_on_every_provider() {
        for (catalog, provider, model) in [
            (catalog(), "kimi", "kimi-k3"),
            (catalog(), "kimi", "kimi-k2.5"),
            (catalog_with_openrouter(), "openrouter", "kimi-k3"),
            (catalog_with_openrouter(), "openrouter", "kimi-k2.6"),
        ] {
            assert_eq!(
                catalog.effective_agent_profile(&ProviderId::new(provider), Some(model)),
                Some(AgentProfileKind::Kimi),
                "{provider}/{model} should use the Kimi profile"
            );
        }
    }

    /// Non-Kimi models on a shared gateway must keep the provider's own
    /// profile — the override is per model, not per provider.
    #[test]
    fn openrouter_non_kimi_models_keep_the_provider_profile() {
        let catalog = catalog_with_openrouter();
        let profile =
            catalog.effective_agent_profile(&ProviderId::new("openrouter"), Some("gpt-5.6-sol"));
        assert_eq!(profile, Some(AgentProfileKind::OpenAi));
    }

    /// The rename must not change what a tool is allowed to do. An exposed
    /// name that fails to resolve would fall back to `Shell` in the CLI gate,
    /// silently demanding approval for reads.
    #[test]
    fn renamed_tools_keep_their_permission_category() {
        let profile = KimiProfile::new("kimi-k3");
        for name in profile.tool_registry().names() {
            let Some(tool) = NativeTool::from_any_name(&name) else {
                continue;
            };
            assert_eq!(
                known_tool_category(&name),
                tool.category(),
                "exposed name '{name}' must categorize as its canonical identity"
            );
        }
        // The specific regression: reads stay reads, not Shell.
        assert_eq!(tool_category("Read"), AgentToolCategory::Read);
        assert_eq!(tool_category("Bash"), AgentToolCategory::Shell);
    }

    #[test]
    fn tools_are_exposed_under_kimi_code_names() {
        let profile = KimiProfile::new("kimi-k3");
        let names = profile.tool_registry().names();
        for expected in ["Read", "Write", "Edit", "Bash", "Grep", "Glob", "FetchURL"] {
            assert!(names.contains(&expected.to_string()), "missing {expected}");
        }
        for canonical in [
            "read_file",
            "write_file",
            "edit_file",
            "shell",
            "grep",
            "glob",
        ] {
            assert!(
                !names.contains(&canonical.to_string()),
                "{canonical} should have been renamed"
            );
        }
        assert!(names.contains(&"TodoList".to_string()));
    }

    /// Tools registered after the profile is constructed must also land in the
    /// Kimi vocabulary, or the model sees a mixed-case tool set.
    #[test]
    fn post_construction_tools_also_use_kimi_names() {
        let mut profile = KimiProfile::new("kimi-k3");
        let factory: SessionFactory = Arc::new(|| panic!("unused"));
        profile.register_subagent_tools(SubAgentSupervisor::new(3), factory, 0);
        profile
            .tool_registry_mut()
            .register(make_use_skill_tool(Arc::new(vec![Skill {
                name:        "demo".into(),
                description: "d".into(),
                template:    "t".into(),
            }])));

        let names = profile.tool_registry().names();
        assert!(names.contains(&"Skill".to_string()), "got {names:?}");
        assert!(!names.contains(&"use_skill".to_string()), "got {names:?}");
        // Deliberately not renamed to Kimi Code's `Agent`: fabro's subagent
        // tools are a supervisor model, not a call-and-return one.
        assert!(names.contains(&"spawn_agent".to_string()), "got {names:?}");
    }

    #[test]
    fn edit_and_write_descriptions_drill_read_before_write() {
        let profile = KimiProfile::new("kimi-k3");
        let describe = |name: &str| {
            profile
                .tool_registry()
                .get(name)
                .unwrap_or_else(|| panic!("{name} should be registered"))
                .definition
                .description
                .clone()
        };

        for name in ["Edit", "Write"] {
            let text = describe(name);
            assert!(
                text.contains("have not been read") || text.contains("has not been read"),
                "{name} should warn about the read-before-write guard"
            );
        }
        assert!(describe("Edit").contains("never reconstruct it from memory"));
        // The shared description is untouched for other profiles.
        assert!(!describe("Read").contains("refuses writes"));
    }

    #[test]
    fn kimi_profile_identity_and_prompt() {
        let profile = KimiProfile::new("kimi-k3")
            .with_provider_id(ProviderId::new("openrouter"))
            .with_catalog(catalog());
        assert_eq!(profile.profile_kind(), AgentProfileKind::Kimi);
        assert_eq!(profile.provider_id(), ProviderId::new("openrouter"));

        let env = MockSandbox::linux();
        let prompt = profile.build_system_prompt(&env, &EnvContext::default(), &[], None, &[]);
        assert!(prompt.contains("You are Kimi"));
        assert!(prompt.contains("# Reading Before Writing"));
        assert!(prompt.contains("<environment>"));
    }
}
