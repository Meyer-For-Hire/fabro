use std::sync::Arc;

use fabro_model::{AgentProfileKind, Catalog, ProviderId};

use super::EnvContext;
use crate::agent_profile::AgentProfile;
use crate::config::NativeToolOptions;
use crate::profiles::{
    self, BaseProfile, EmbeddedPrompt, ProfileDeps, impl_base_profile_accessors,
};
use crate::sandbox::Sandbox;
use crate::skills::Skill;
use crate::tool_registry::ToolRegistry;
use crate::tools::{
    WEB_SEARCH_TOOL_NAME, make_edit_file_tool, make_list_dir_tool, make_read_many_files_tool,
    register_core_tools,
};

const CORE_PROMPT: &str = include_str!("prompts/gemini.md.j2");

pub struct GeminiProfile {
    base: BaseProfile,
}

impl GeminiProfile {
    #[must_use]
    pub fn new(model: impl Into<String>) -> Self {
        let deps =
            ProfileDeps::standalone(NativeToolOptions::for_profile(AgentProfileKind::Gemini));
        Self::with_native_tools(model, &deps)
    }

    pub(crate) fn with_native_tools(model: impl Into<String>, deps: &ProfileDeps) -> Self {
        let mut registry = ToolRegistry::new();

        register_core_tools(&mut registry, &deps.options, deps.summarizer.clone());
        registry.register(make_edit_file_tool());
        registry.register(make_read_many_files_tool());
        registry.register(make_list_dir_tool());

        Self {
            base: BaseProfile {
                profile_kind: AgentProfileKind::Gemini,
                provider_id: ProviderId::gemini(),
                model: model.into(),
                catalog: None,
                registry,
            },
        }
    }

    /// Override the provider ID while retaining the adapter/profile behavior.
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

impl AgentProfile for GeminiProfile {
    impl_base_profile_accessors!();

    fn build_system_prompt(
        &self,
        env: &dyn Sandbox,
        env_context: &EnvContext,
        memory: &[String],
        user_instructions: Option<&str>,
        skills: &[Skill],
    ) -> String {
        let has_web_search = self.base.registry.get(WEB_SEARCH_TOOL_NAME).is_some();
        let template = EmbeddedPrompt::new("gemini.md.j2", CORE_PROMPT)
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

    use super::*;
    use crate::subagent::{SessionFactory, SubAgentSupervisor};
    use crate::test_support::MockSandbox;

    fn test_catalog() -> Arc<Catalog> {
        Arc::new(Catalog::from_builtin().unwrap())
    }

    #[test]
    fn gemini_profile_identity() {
        let profile = GeminiProfile::new("gemini-2.0-flash");
        assert_eq!(profile.profile_kind(), AgentProfileKind::Gemini);
        assert_eq!(profile.provider_id(), ProviderId::gemini());
        assert_eq!(profile.model(), "gemini-2.0-flash");
    }

    #[test]
    fn gemini_context_window_from_catalog() {
        let profile = GeminiProfile::new("gemini-3.1-pro-preview").with_catalog(test_catalog());
        assert_eq!(profile.context_window_size(), 1_048_576);
    }

    #[test]
    fn gemini_system_prompt_contains_identity() {
        let profile = GeminiProfile::new("gemini-2.0-flash");
        let env = MockSandbox::linux();
        let prompt = profile.build_system_prompt(&env, &EnvContext::default(), &[], None, &[]);
        assert!(prompt.contains("You are Gemini CLI"));
        assert!(prompt.contains("solving bugs"));
        assert!(prompt.contains("adding new functionality"));
        assert!(prompt.contains("refactoring code"));
        assert!(prompt.contains("explaining code"));
    }

    #[test]
    fn gemini_system_prompt_contains_tool_guidance() {
        let profile = GeminiProfile::new("gemini-2.0-flash");
        let env = MockSandbox::linux();
        let prompt = profile.build_system_prompt(&env, &EnvContext::default(), &[], None, &[]);
        assert!(prompt.contains("read_file"));
        assert!(prompt.contains("read_many_files"));
        assert!(prompt.contains("edit_file"));
        assert!(prompt.contains("write_file"));
        assert!(prompt.contains("shell"));
        assert!(prompt.contains("grep"));
        assert!(prompt.contains("glob"));
        assert!(prompt.contains("list_dir"));
        assert!(!prompt.contains("web_search"));
        assert!(prompt.contains("web_fetch"));
        assert!(prompt.contains("Default timeout is 10 seconds"));
    }

    #[test]
    fn gemini_system_prompt_contains_memory_convention() {
        let profile = GeminiProfile::new("gemini-2.0-flash");
        let env = MockSandbox::linux();
        let prompt = profile.build_system_prompt(&env, &EnvContext::default(), &[], None, &[]);
        assert!(prompt.contains("GEMINI.md"));
        assert!(prompt.contains("AGENTS.md"));
    }

    #[test]
    fn gemini_system_prompt_contains_coding_best_practices() {
        let profile = GeminiProfile::new("gemini-2.0-flash");
        let env = MockSandbox::linux();
        let prompt = profile.build_system_prompt(&env, &EnvContext::default(), &[], None, &[]);
        assert!(prompt.contains("clean, maintainable code"));
        assert!(prompt.contains("Handle errors appropriately"));
        assert!(prompt.contains("existing code conventions"));
    }

    #[test]
    fn gemini_system_prompt_contains_env_context() {
        let profile = GeminiProfile::new("gemini-2.0-flash");
        let env = MockSandbox::linux();
        let prompt = profile.build_system_prompt(&env, &EnvContext::default(), &[], None, &[]);
        assert!(prompt.contains("<environment>"));
        assert!(prompt.contains("linux"));
    }

    #[test]
    fn gemini_tools_registered() {
        let profile = GeminiProfile::new("gemini-2.0-flash");
        let names = profile.tool_registry().names();
        assert_eq!(names.len(), 9);
        assert!(names.contains(&"read_file".to_string()));
        assert!(names.contains(&"read_many_files".to_string()));
        assert!(names.contains(&"write_file".to_string()));
        assert!(names.contains(&"edit_file".to_string()));
        assert!(names.contains(&"shell".to_string()));
        assert!(names.contains(&"grep".to_string()));
        assert!(names.contains(&"glob".to_string()));
        assert!(names.contains(&"list_dir".to_string()));
        assert!(!names.contains(&"web_search".to_string()));
        assert!(names.contains(&"web_fetch".to_string()));
    }

    #[test]
    fn gemini_subagent_tools_registered() {
        let mut profile = GeminiProfile::new("gemini-2.0-flash");
        let supervisor = SubAgentSupervisor::new(3);
        let factory: SessionFactory = Arc::new(|| {
            panic!("should not be called");
        });
        profile.register_subagent_tools(supervisor, factory, 0);
        let names = profile.tool_registry().names();
        assert_eq!(names.len(), 13);
        assert!(names.contains(&"spawn_agent".to_string()));
        assert!(names.contains(&"send_input".to_string()));
        assert!(names.contains(&"wait".to_string()));
        assert!(names.contains(&"close_agent".to_string()));
    }
}
