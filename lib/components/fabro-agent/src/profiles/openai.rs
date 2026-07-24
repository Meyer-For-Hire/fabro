use std::sync::Arc;

use fabro_model::{AgentProfileKind, Catalog, CodecKind, ProviderId};

use super::EnvContext;
use crate::agent_profile::AgentProfile;
use crate::apply_patch;
use crate::config::NativeToolOptions;
use crate::profiles::{BaseProfile, assemble_system_prompt, render_prompt};
use crate::sandbox::Sandbox;
use crate::skills::Skill;
use crate::todo_runtime::TodoRuntime;
use crate::todo_tools::make_update_plan_tool;
use crate::tool_registry::ToolRegistry;
use crate::tools::{self, WebFetchSummarizer, register_core_tools};

const CORE_PROMPT: &str = include_str!("prompts/openai.md");
const APPLY_PATCH_SECTION: &str = include_str!("prompts/openai_apply_patch.md");
const EDIT_FILE_SECTION: &str = include_str!("prompts/openai_edit_file.md");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FileEditToolKind {
    ApplyPatch,
    EditFile,
}

impl FileEditToolKind {
    fn for_codec(codec: CodecKind) -> Self {
        if codec == CodecKind::OpenAiResponses {
            Self::ApplyPatch
        } else {
            Self::EditFile
        }
    }
}

pub struct OpenAiProfile {
    base:           BaseProfile,
    file_edit_tool: FileEditToolKind,
}

impl OpenAiProfile {
    #[must_use]
    pub fn new(model: impl Into<String>) -> Self {
        let options = NativeToolOptions::for_profile(AgentProfileKind::OpenAi);
        Self::with_native_tools(model, &options, None)
    }

    pub(crate) fn with_native_tools(
        model: impl Into<String>,
        options: &NativeToolOptions,
        summarizer: Option<WebFetchSummarizer>,
    ) -> Self {
        let mut registry = ToolRegistry::new();

        register_core_tools(&mut registry, options, summarizer);
        registry.register(apply_patch::make_apply_patch_tool());
        // Codex-compatible `update_plan` is OpenAI-only.
        let todo_runtime = Arc::new(TodoRuntime::new());
        registry.register(make_update_plan_tool(todo_runtime));

        Self {
            base:           BaseProfile {
                profile_kind: AgentProfileKind::OpenAi,
                provider_id: ProviderId::openai(),
                model: model.into(),
                catalog: None,
                registry,
            },
            file_edit_tool: FileEditToolKind::ApplyPatch,
        }
    }

    /// Override the provider ID while retaining the adapter/profile behavior.
    #[must_use]
    pub fn with_provider_id(mut self, provider_id: ProviderId) -> Self {
        self.base.provider_id = provider_id;
        self.configure_file_edit_tool();
        self
    }

    #[must_use]
    pub fn with_catalog(mut self, catalog: Arc<Catalog>) -> Self {
        self.base.catalog = Some(catalog);
        self.configure_file_edit_tool();
        self
    }

    fn configure_file_edit_tool(&mut self) {
        let Some(codec) = self.base.catalog.as_ref().and_then(|catalog| {
            catalog.effective_codec(&self.base.provider_id, Some(&self.base.model))
        }) else {
            return;
        };
        let desired = FileEditToolKind::for_codec(codec);
        if desired == self.file_edit_tool {
            return;
        }

        match desired {
            FileEditToolKind::ApplyPatch => {
                self.base.registry.unregister("edit_file");
                self.base
                    .registry
                    .register(apply_patch::make_apply_patch_tool());
            }
            FileEditToolKind::EditFile => {
                self.base.registry.unregister("apply_patch");
                self.base.registry.register(tools::make_edit_file_tool());
            }
        }
        self.file_edit_tool = desired;
    }

    fn provider_display_name(&self) -> String {
        self.base
            .catalog
            .as_ref()
            .and_then(|catalog| catalog.provider(&self.base.provider_id))
            .map_or_else(
                || self.base.provider_id.display_name(),
                |provider| provider.display_name.clone(),
            )
    }
}

impl AgentProfile for OpenAiProfile {
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
        let (file_edit_tool_name, file_edit_failure_guidance) = match self.file_edit_tool {
            // One-line failure hints stay inline; the multi-line usage blocks
            // they pair with live in prompts/openai_{apply_patch,edit_file}.md.
            FileEditToolKind::ApplyPatch => (
                "apply_patch",
                "- When apply_patch fails, use the error text to construct a corrected patch. \
Re-read the target file if you need fresh context.",
            ),
            FileEditToolKind::EditFile => (
                "edit_file",
                "- When edit_file fails, use the error text to construct a corrected exact \
replacement. Re-read the target file if you need fresh context.",
            ),
        };
        let file_edit_tool_guidance = match self.file_edit_tool {
            FileEditToolKind::ApplyPatch => APPLY_PATCH_SECTION,
            FileEditToolKind::EditFile => EDIT_FILE_SECTION,
        };
        let web_search_guidance = if self
            .base
            .registry
            .get(tools::WEB_SEARCH_TOOL_NAME)
            .is_some()
        {
            "## web_search
Search the web using Brave Search. Returns titles, URLs, and descriptions.

"
        } else {
            ""
        };
        let core_prompt = render_prompt(CORE_PROMPT, &[
            ("provider_name", &self.provider_display_name()),
            ("file_edit_tool_name", file_edit_tool_name),
            ("file_edit_failure_guidance", file_edit_failure_guidance),
            (
                "file_edit_tool_guidance",
                file_edit_tool_guidance.trim_end(),
            ),
            ("web_search_section", web_search_guidance),
        ]);

        assemble_system_prompt(
            &core_prompt,
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

    use super::*;
    use crate::subagent::{SessionFactory, SubAgentSupervisor};
    use crate::test_support::MockSandbox;

    fn test_catalog() -> Arc<Catalog> {
        Arc::new(Catalog::from_builtin().unwrap())
    }

    #[test]
    fn openai_profile_identity() {
        let profile = OpenAiProfile::new("o3-mini");
        assert_eq!(profile.profile_kind(), AgentProfileKind::OpenAi);
        assert_eq!(profile.provider_id(), ProviderId::openai());
        assert_eq!(profile.model(), "o3-mini");
    }

    #[test]
    fn openai_system_prompt_contains_env_context() {
        let profile = OpenAiProfile::new("o3-mini");
        let env = MockSandbox::linux();
        let prompt = profile.build_system_prompt(&env, &EnvContext::default(), &[], None, &[]);
        assert!(prompt.contains("You are a coding agent powered by openai"));
        assert!(prompt.contains("<environment>"));
        assert!(prompt.contains("linux"));
        assert!(prompt.contains("freeform tool"));
        assert!(prompt.contains("*** Begin Patch"));
    }

    #[test]
    fn openai_system_prompt_contains_tool_guidance() {
        let profile = OpenAiProfile::new("o3-mini");
        let env = MockSandbox::linux();
        let prompt = profile.build_system_prompt(&env, &EnvContext::default(), &[], None, &[]);
        assert!(prompt.contains("read_file"));
        assert!(prompt.contains("apply_patch"));
        assert!(prompt.contains("write_file"));
        assert!(prompt.contains("shell"));
        assert!(prompt.contains("grep"));
        assert!(prompt.contains("glob"));
        assert!(prompt.contains("timeout_ms"));
        assert!(!prompt.contains("## web_search"));
    }

    #[test]
    fn openai_system_prompt_contains_coding_best_practices() {
        let profile = OpenAiProfile::new("o3-mini");
        let env = MockSandbox::linux();
        let prompt = profile.build_system_prompt(&env, &EnvContext::default(), &[], None, &[]);
        assert!(prompt.contains("clean, maintainable code"));
        assert!(prompt.contains("existing code conventions"));
    }

    #[test]
    fn openai_system_prompt_matches_codex_incremental_plan_guidance() {
        let profile = OpenAiProfile::new("gpt-5.5");
        let env = MockSandbox::linux();
        let prompt = profile.build_system_prompt(&env, &EnvContext::default(), &[], None, &[]);
        assert!(prompt.contains(
            "update item statuses incrementally as each item is completed rather than \
                 marking every item done only at the end"
        ));
    }

    #[test]
    fn openai_system_prompt_includes_memory() {
        let profile = OpenAiProfile::new("o3-mini");
        let env = MockSandbox::linux();
        let docs = vec!["# Project README".into(), "# CONTRIBUTING guide".into()];
        let prompt = profile.build_system_prompt(&env, &EnvContext::default(), &docs, None, &[]);
        assert!(prompt.contains("# Project README"));
        assert!(prompt.contains("# CONTRIBUTING guide"));
    }

    #[test]
    fn openai_system_prompt_includes_user_instructions() {
        let profile = OpenAiProfile::new("o3-mini");
        let env = MockSandbox::linux();
        let prompt = profile.build_system_prompt(
            &env,
            &EnvContext::default(),
            &[],
            Some("Always write tests first"),
            &[],
        );
        assert!(prompt.contains("Always write tests first"));
        assert!(prompt.contains("# User Instructions"));
    }

    #[test]
    fn openai_subagent_tools_registered() {
        let mut profile = OpenAiProfile::new("o3-mini");
        assert_eq!(profile.tool_registry().names().len(), 8);

        let supervisor = SubAgentSupervisor::new(3);
        let factory: SessionFactory = Arc::new(|| panic!("should not be called in test"));
        profile.register_subagent_tools(supervisor, factory, 0);
        assert_eq!(profile.tool_registry().names().len(), 12);
    }

    #[test]
    fn openai_tools_registered() {
        let profile = OpenAiProfile::new("o3-mini");
        let names = profile.tool_registry().names();
        assert_eq!(names.len(), 8);
        assert!(names.contains(&"read_file".to_string()));
        assert!(names.contains(&"write_file".to_string()));
        assert!(names.contains(&"shell".to_string()));
        assert!(names.contains(&"grep".to_string()));
        assert!(names.contains(&"glob".to_string()));
        assert!(names.contains(&"apply_patch".to_string()));
        assert!(!names.contains(&"web_search".to_string()));
        assert!(names.contains(&"web_fetch".to_string()));
        assert!(names.contains(&"update_plan".to_string()));

        let apply_patch = profile.tool_registry().get("apply_patch").unwrap();
        assert!(apply_patch.definition.is_custom());
    }

    #[test]
    fn openai_profile_excludes_anthropic_task_tools() {
        let profile = OpenAiProfile::new("o3-mini");
        let names = profile.tool_registry().names();
        assert!(!names.contains(&"TaskCreate".to_string()));
        assert!(!names.contains(&"TaskUpdate".to_string()));
        assert!(!names.contains(&"TaskList".to_string()));
    }

    #[test]
    fn kimi_provider_prompt_uses_catalog_display_name() {
        let profile = OpenAiProfile::new("kimi-k2.5")
            .with_provider_id(ProviderId::new("kimi"))
            .with_catalog(test_catalog());
        let env = MockSandbox::linux();
        let prompt = profile.build_system_prompt(&env, &EnvContext::default(), &[], None, &[]);
        assert!(prompt.contains("powered by Kimi"));
        assert!(!prompt.contains("powered by OpenAI"));
    }

    #[test]
    fn openai_compatible_profile_uses_json_schema_edit_tool() {
        let profile = OpenAiProfile::new("kimi-k2.5")
            .with_provider_id(ProviderId::new("kimi"))
            .with_catalog(test_catalog());

        let names = profile.tool_registry().names();
        assert!(names.contains(&"edit_file".to_string()));
        assert!(!names.contains(&"apply_patch".to_string()));

        let edit_file = profile.tool_registry().get("edit_file").unwrap();
        assert!(!edit_file.definition.is_custom());
        assert_eq!(edit_file.definition.parameters["type"], "object");
        for definition in profile.tool_registry().definitions() {
            assert_eq!(
                definition.parameters["type"], "object",
                "tool '{}' must use an object parameter schema",
                definition.name
            );
        }

        let env = MockSandbox::linux();
        let prompt = profile.build_system_prompt(&env, &EnvContext::default(), &[], None, &[]);
        assert!(prompt.contains("## edit_file"));
        assert!(!prompt.contains("## apply_patch"));
        assert!(!prompt.contains("freeform tool"));
    }

    #[test]
    fn file_edit_tool_selection_is_builder_order_independent() {
        let profile = OpenAiProfile::new("kimi-k2.5")
            .with_catalog(test_catalog())
            .with_provider_id(ProviderId::new("kimi"));

        assert!(profile.tool_registry().get("edit_file").is_some());
        assert!(profile.tool_registry().get("apply_patch").is_none());
    }

    #[test]
    fn zai_provider_prompt_uses_catalog_display_name() {
        let profile = OpenAiProfile::new("glm-4.7")
            .with_provider_id(ProviderId::new("zai"))
            .with_catalog(test_catalog());
        let env = MockSandbox::linux();
        let prompt = profile.build_system_prompt(&env, &EnvContext::default(), &[], None, &[]);
        assert!(prompt.contains("powered by Z.ai"));
    }

    #[test]
    fn minimax_provider_prompt_uses_catalog_display_name() {
        let profile = OpenAiProfile::new("minimax-m2.5")
            .with_provider_id(ProviderId::new("minimax"))
            .with_catalog(test_catalog());
        let env = MockSandbox::linux();
        let prompt = profile.build_system_prompt(&env, &EnvContext::default(), &[], None, &[]);
        assert!(prompt.contains("powered by MiniMax"));
    }

    #[test]
    fn inception_provider_prompt_uses_catalog_display_name() {
        let profile = OpenAiProfile::new("mercury-2")
            .with_provider_id(ProviderId::new("inception"))
            .with_catalog(test_catalog());
        let env = MockSandbox::linux();
        let prompt = profile.build_system_prompt(&env, &EnvContext::default(), &[], None, &[]);
        assert!(prompt.contains("powered by Inception"));
    }
}
