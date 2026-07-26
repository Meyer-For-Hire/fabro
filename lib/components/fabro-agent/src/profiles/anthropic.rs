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
use crate::todo_tools::{
    make_task_create_tool, make_task_get_tool, make_task_list_tool, make_task_update_tool,
};
use crate::tool_registry::ToolRegistry;
use crate::tools::{WEB_SEARCH_TOOL_NAME, make_edit_file_tool, register_core_tools};

pub struct AnthropicProfile {
    base: BaseProfile,
}

const CORE_PROMPT: &str = include_str!("prompts/anthropic.md.j2");

impl AnthropicProfile {
    #[must_use]
    pub fn new(model: impl Into<String>) -> Self {
        let deps =
            ProfileDeps::standalone(NativeToolOptions::for_profile(AgentProfileKind::Anthropic));
        Self::with_native_tools(model, &deps)
    }

    pub(crate) fn with_native_tools(model: impl Into<String>, deps: &ProfileDeps) -> Self {
        let mut registry = ToolRegistry::new();

        register_core_tools(&mut registry, &deps.options, deps.summarizer.clone());
        registry.register(make_edit_file_tool());
        // Task tools scope their list by `root_session_id`, so a root session
        // and its children address one logical list. They must therefore
        // resolve it through the one runtime the builder shares between them.
        let todo_runtime = Arc::clone(&deps.todo_runtime);
        registry.register(make_task_create_tool(todo_runtime.clone()));
        registry.register(make_task_update_tool(todo_runtime.clone()));
        registry.register(make_task_get_tool(todo_runtime.clone()));
        registry.register(make_task_list_tool(todo_runtime));

        Self {
            base: BaseProfile {
                profile_kind: AgentProfileKind::Anthropic,
                provider_id: ProviderId::anthropic(),
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

impl AgentProfile for AnthropicProfile {
    impl_base_profile_accessors!();

    fn build_system_prompt(
        &self,
        env: &dyn Sandbox,
        env_context: &EnvContext,
        memory: &[String],
        user_instructions: Option<&str>,
        skills: &[Skill],
    ) -> String {
        let has_spawn_agent = self.base.registry.get("spawn_agent").is_some();
        let has_web_search = self.base.registry.get(WEB_SEARCH_TOOL_NAME).is_some();
        let template = EmbeddedPrompt::new("anthropic.md.j2", CORE_PROMPT)
            .with_bool("has_spawn_agent", has_spawn_agent)
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
    fn anthropic_profile_identity() {
        let profile = AnthropicProfile::new("claude-sonnet-4-20250514");
        assert_eq!(profile.profile_kind(), AgentProfileKind::Anthropic);
        assert_eq!(profile.provider_id(), ProviderId::anthropic());
        assert_eq!(profile.model(), "claude-sonnet-4-20250514");
    }

    #[test]
    fn anthropic_context_window_from_catalog() {
        let profile = AnthropicProfile::new("claude-opus-4-6").with_catalog(test_catalog());
        assert_eq!(profile.context_window_size(), 1_000_000);

        let profile = AnthropicProfile::new("claude-sonnet-4-6").with_catalog(test_catalog());
        assert_eq!(profile.context_window_size(), 200_000);
    }

    #[test]
    fn anthropic_knowledge_cutoff_from_catalog() {
        let profile = AnthropicProfile::new("claude-opus-4-6").with_catalog(test_catalog());
        assert_eq!(profile.knowledge_cutoff(), Some("May 2025".to_string()));
    }

    #[test]
    fn anthropic_system_prompt_contains_env_context() {
        let profile = AnthropicProfile::new("claude-sonnet-4-20250514");
        let env = MockSandbox::linux();
        let prompt = profile.build_system_prompt(&env, &EnvContext::default(), &[], None, &[]);
        assert!(prompt.contains("You are Claude, an AI coding assistant made by Anthropic"));
        assert!(prompt.contains("<environment>"));
        assert!(prompt.contains("linux"));
        assert!(prompt.contains("/home/test"));
        assert!(prompt.contains("# Using your tools"));
        assert!(
            prompt.contains("Do NOT use the shell tool to run commands when a relevant dedicated tool is provided"),
            "prompt should prefer dedicated tools"
        );
        assert!(
            prompt.contains("Use TaskUpdate to keep task status current"),
            "prompt should mention real task management tools"
        );
        assert!(
            !prompt.contains("## read_file"),
            "prompt should rely on tool descriptions for detailed per-tool usage"
        );
        assert!(
            prompt.contains("Write clean, maintainable code"),
            "prompt should contain coding best practices"
        );
        assert!(
            !prompt.contains("web_search"),
            "prompt should omit guidance for unavailable tools"
        );
        assert!(
            prompt.contains("web_fetch"),
            "prompt should contain web_fetch guidance"
        );
    }

    #[test]
    fn anthropic_system_prompt_uses_claude_code_style_sections() {
        let profile = AnthropicProfile::new("claude-sonnet-4-20250514");
        let env = MockSandbox::linux();
        let prompt = profile.build_system_prompt(&env, &EnvContext::default(), &[], None, &[]);

        assert!(prompt.contains("# System"));
        assert!(prompt.contains("# Doing tasks"));
        assert!(prompt.contains("# Executing actions with care"));
        assert!(prompt.contains("# Using your tools"));
        assert!(prompt.contains("# Tone and style"));
        assert!(
            prompt.contains("Break down and manage your work with the TaskCreate tool"),
            "prompt should tell Anthropic models to use TaskCreate for task management"
        );
        assert!(
            prompt.contains("Mark each task as completed as soon as you are done"),
            "prompt should discourage batched task completion"
        );
    }

    #[test]
    fn anthropic_system_prompt_contains_communication_and_safety_guidance() {
        let profile = AnthropicProfile::new("claude-sonnet-4-20250514");
        let env = MockSandbox::linux();
        let prompt = profile.build_system_prompt(&env, &EnvContext::default(), &[], None, &[]);

        assert!(
            prompt.contains("Before your first tool call, briefly state what you're about to do")
        );
        assert!(prompt.contains("Do not expose internal deliberation"));
        assert!(prompt.contains("Do not create planning documents unless the user asks"));
        assert!(prompt.contains("ask the user before proceeding"));
        assert!(prompt.contains("read or inspect it first"));
        assert!(prompt.contains("Report outcomes faithfully"));
    }

    #[test]
    fn anthropic_system_prompt_includes_subagent_guidance_only_when_registered() {
        let env = MockSandbox::linux();
        let profile = AnthropicProfile::new("claude-sonnet-4-20250514");
        let prompt = profile.build_system_prompt(&env, &EnvContext::default(), &[], None, &[]);
        assert!(!prompt.contains("Subagents are valuable for independent work"));

        let mut profile = AnthropicProfile::new("claude-sonnet-4-20250514");
        let supervisor = SubAgentSupervisor::new(3);
        let factory: SessionFactory = Arc::new(|| {
            panic!("should not be called in test");
        });
        profile.register_subagent_tools(supervisor, factory, 0);
        let prompt = profile.build_system_prompt(&env, &EnvContext::default(), &[], None, &[]);

        assert!(prompt.contains("Subagents are valuable for independent work"));
        assert!(prompt.contains("avoid duplicating work"));
        assert!(prompt.contains("wait for their results and synthesize them"));
    }

    #[test]
    fn anthropic_system_prompt_includes_memory() {
        let profile = AnthropicProfile::new("claude-sonnet-4-20250514");
        let env = MockSandbox::linux();
        let docs = vec!["# Project README".into(), "# CONTRIBUTING guide".into()];
        let prompt = profile.build_system_prompt(&env, &EnvContext::default(), &docs, None, &[]);
        assert!(prompt.contains("# Project README"));
        assert!(prompt.contains("# CONTRIBUTING guide"));
    }

    #[test]
    fn anthropic_system_prompt_includes_env_context() {
        let profile = AnthropicProfile::new("claude-opus-4-6");
        let env = MockSandbox::linux();
        let ctx = EnvContext {
            git_branch:         Some("feature-branch".into()),
            is_git_repo:        true,
            current_date:       "2026-02-20".into(),
            model:              "claude-opus-4-6".into(),
            knowledge_cutoff:   "May 2025".into(),
            git_status_short:   None,
            git_recent_commits: None,
        };
        let prompt = profile.build_system_prompt(&env, &ctx, &[], None, &[]);
        assert!(prompt.contains("Git branch: feature-branch"));
        assert!(prompt.contains("Is git repository: true"));
        assert!(prompt.contains("Today's date: 2026-02-20"));
        assert!(prompt.contains("Model: claude-opus-4-6"));
        assert!(prompt.contains("Knowledge cutoff: May 2025"));
    }

    #[test]
    fn anthropic_system_prompt_includes_user_instructions() {
        let profile = AnthropicProfile::new("claude-opus-4-6");
        let env = MockSandbox::linux();
        let ctx = EnvContext::default();
        let prompt =
            profile.build_system_prompt(&env, &ctx, &[], Some("Always write tests first"), &[]);
        assert!(prompt.contains("Always write tests first"));
        assert!(prompt.contains("# User Instructions"));
    }

    #[test]
    fn anthropic_tools_registered() {
        let profile = AnthropicProfile::new("claude-sonnet-4-20250514");
        let names = profile.tool_registry().names();
        assert_eq!(names.len(), 11);
        assert!(names.contains(&"read_file".to_string()));
        assert!(names.contains(&"write_file".to_string()));
        assert!(names.contains(&"edit_file".to_string()));
        assert!(names.contains(&"shell".to_string()));
        assert!(names.contains(&"grep".to_string()));
        assert!(names.contains(&"glob".to_string()));
        assert!(!names.contains(&"web_search".to_string()));
        assert!(names.contains(&"web_fetch".to_string()));
        assert!(names.contains(&"TaskCreate".to_string()));
        assert!(names.contains(&"TaskUpdate".to_string()));
        assert!(names.contains(&"TaskGet".to_string()));
        assert!(names.contains(&"TaskList".to_string()));
    }

    #[test]
    fn anthropic_profile_excludes_openai_update_plan() {
        let profile = AnthropicProfile::new("claude-sonnet-4-20250514");
        let names = profile.tool_registry().names();
        assert!(!names.contains(&"update_plan".to_string()));
    }

    #[test]
    fn anthropic_register_subagent_tools() {
        let mut profile = AnthropicProfile::new("claude-sonnet-4-20250514");
        assert_eq!(profile.tool_registry().names().len(), 11);

        let supervisor = SubAgentSupervisor::new(3);
        let factory: SessionFactory = Arc::new(|| {
            panic!("should not be called in test");
        });

        profile.register_subagent_tools(supervisor, factory, 0);

        let names = profile.tool_registry().names();
        assert_eq!(names.len(), 15, "should have 11 base + 4 subagent tools");
        assert!(names.contains(&"spawn_agent".to_string()));
        assert!(names.contains(&"send_input".to_string()));
        assert!(names.contains(&"wait".to_string()));
        assert!(names.contains(&"close_agent".to_string()));
    }
}
