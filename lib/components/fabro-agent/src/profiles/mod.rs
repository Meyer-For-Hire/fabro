use std::collections::HashMap;
use std::sync::Arc;

use fabro_model::{AgentProfileKind, Catalog, ProviderId};

pub mod anthropic;
pub mod gemini;
pub mod openai;

pub use anthropic::AnthropicProfile;
pub use gemini::GeminiProfile;
pub use openai::OpenAiProfile;

use crate::agent_profile::AgentProfile;
use crate::config::{NativeToolOptions, ToolSecrets};
use crate::sandbox::Sandbox;
use crate::skills::{Skill, format_skills_prompt_section};
use crate::tool_registry::ToolRegistry;
use crate::tools::WebFetchSummarizer;

/// Builds a provider profile and its native tools from one configuration.
///
/// Native tool options must be supplied before [`Self::build`] because their
/// values are captured by tool executors during profile construction.
/// [`Self::build`] borrows, so one configured builder can outfit both a root
/// session and every child session it spawns with an identical tool set.
#[derive(Clone)]
pub struct AgentProfileBuilder {
    profile_kind:        AgentProfileKind,
    provider_id:         ProviderId,
    model:               String,
    catalog:             Arc<Catalog>,
    native_tool_options: NativeToolOptions,
    summarizer:          Option<WebFetchSummarizer>,
}

impl AgentProfileBuilder {
    #[must_use]
    pub fn new(
        profile_kind: AgentProfileKind,
        provider_id: ProviderId,
        model: impl Into<String>,
        catalog: Arc<Catalog>,
    ) -> Self {
        Self {
            profile_kind,
            provider_id,
            model: model.into(),
            catalog,
            native_tool_options: NativeToolOptions::for_profile(profile_kind),
            summarizer: None,
        }
    }

    #[must_use]
    pub fn with_tool_secrets(mut self, secrets: ToolSecrets) -> Self {
        self.native_tool_options.secrets = secrets;
        self
    }

    #[must_use]
    pub fn with_web_fetch_summarizer(mut self, summarizer: Option<WebFetchSummarizer>) -> Self {
        self.summarizer = summarizer;
        self
    }

    #[must_use]
    pub fn build(&self) -> Box<dyn AgentProfile> {
        let model = self.model.as_str();
        let options = &self.native_tool_options;
        let summarizer = self.summarizer.clone();
        match self.profile_kind {
            AgentProfileKind::OpenAi => Box::new(
                OpenAiProfile::with_native_tools(model, options, summarizer)
                    .with_provider_id(self.provider_id.clone())
                    .with_catalog(Arc::clone(&self.catalog)),
            ),
            AgentProfileKind::Gemini => Box::new(
                GeminiProfile::with_native_tools(model, options, summarizer)
                    .with_provider_id(self.provider_id.clone())
                    .with_catalog(Arc::clone(&self.catalog)),
            ),
            AgentProfileKind::Anthropic => Box::new(
                AnthropicProfile::with_native_tools(model, options, summarizer)
                    .with_provider_id(self.provider_id.clone())
                    .with_catalog(Arc::clone(&self.catalog)),
            ),
        }
    }
}

/// Common fields shared by all provider profiles.
///
/// Each concrete profile embeds this struct and delegates `profile_kind()`,
/// `model()`, `tool_registry()`, and `tool_registry_mut()` to it.
pub struct BaseProfile {
    pub profile_kind: AgentProfileKind,
    pub provider_id:  ProviderId,
    pub model:        String,
    pub catalog:      Option<Arc<Catalog>>,
    pub registry:     ToolRegistry,
}

/// Additional context for building environment blocks
#[derive(Default)]
pub struct EnvContext {
    pub git_branch:         Option<String>,
    pub is_git_repo:        bool,
    pub current_date:       String,
    pub model:              String,
    pub knowledge_cutoff:   String,
    pub git_status_short:   Option<String>,
    pub git_recent_commits: Option<String>,
}

/// A checked-in MiniJinja system-prompt template and its typed inputs.
///
/// The environment block is supplied by [`assemble_system_prompt`] and cannot
/// be overridden by callers.
pub struct EmbeddedPrompt {
    name:   &'static str,
    source: &'static str,
    inputs: HashMap<String, toml::Value>,
}

impl EmbeddedPrompt {
    #[must_use]
    pub fn new(name: &'static str, source: &'static str) -> Self {
        Self {
            name,
            source,
            inputs: HashMap::new(),
        }
    }

    #[must_use]
    pub fn with_string(mut self, name: &'static str, value: impl Into<String>) -> Self {
        self.inputs
            .insert(name.to_string(), toml::Value::String(value.into()));
        self
    }

    #[must_use]
    pub fn with_bool(mut self, name: &'static str, value: bool) -> Self {
        self.inputs
            .insert(name.to_string(), toml::Value::Boolean(value));
        self
    }

    fn render(mut self, env_block: String) -> String {
        self.inputs
            .insert("env_block".to_string(), toml::Value::String(env_block));
        let ctx = fabro_template::TemplateContext::new().with_inputs(self.inputs);
        fabro_template::render_named(self.name, self.source, &ctx).unwrap_or_else(|err| {
            panic!(
                "embedded prompt template '{}' failed to render: {err}",
                self.name
            )
        })
    }
}

/// Assembles a complete system prompt from an embedded template and the
/// standard trailing sections.
///
/// # Panics
/// Panics if a checked-in template is invalid or references an input its
/// caller did not supply. Tests render every conditional template variant, so
/// this indicates a programmer error rather than a recoverable runtime error.
#[must_use]
pub fn assemble_system_prompt(
    template: EmbeddedPrompt,
    env: &dyn Sandbox,
    env_context: &EnvContext,
    memory: &[String],
    user_instructions: Option<&str>,
    skills: &[Skill],
) -> String {
    let env_block = build_env_context_block_with(env, env_context);
    let prompt = template.render(env_block);

    let docs_section = if memory.is_empty() {
        String::new()
    } else {
        format!("\n\n{}", memory.join("\n\n"))
    };
    let skills_section = {
        let s = format_skills_prompt_section(skills);
        if s.is_empty() {
            String::new()
        } else {
            format!("\n\n{s}")
        }
    };
    let user_section = match user_instructions {
        Some(instructions) => format!("\n\n# User Instructions\n{instructions}"),
        None => String::new(),
    };

    format!("{prompt}{docs_section}{skills_section}{user_section}")
}

#[cfg(test)]
#[must_use]
pub fn build_env_context_block(env: &dyn Sandbox) -> String {
    build_env_context_block_with(env, &EnvContext::default())
}

#[must_use]
pub fn build_env_context_block_with(env: &dyn Sandbox, ctx: &EnvContext) -> String {
    let mut lines = vec![
        "<environment>".to_string(),
        format!("Working directory: {}", env.working_directory()),
        format!("Is git repository: {}", ctx.is_git_repo),
    ];

    if let Some(ref branch) = ctx.git_branch {
        lines.push(format!("Git branch: {branch}"));
    }

    lines.push(format!("Platform: {}", env.platform()));
    lines.push(format!("OS version: {}", env.os_version()));

    if !ctx.current_date.is_empty() {
        lines.push(format!("Today's date: {}", ctx.current_date));
    }
    if !ctx.model.is_empty() {
        lines.push(format!("Model: {}", ctx.model));
    }
    if !ctx.knowledge_cutoff.is_empty() {
        lines.push(format!("Knowledge cutoff: {}", ctx.knowledge_cutoff));
    }

    if let Some(ref status) = ctx.git_status_short {
        lines.push(format!("Git status:\n{status}"));
    }
    if let Some(ref commits) = ctx.git_recent_commits {
        lines.push(format!("Recent commits:\n{commits}"));
    }

    lines.push("</environment>".to_string());
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use fabro_llm::types::ToolDefinition;

    use super::*;
    use crate::subagent::{SessionFactory, SubAgentSupervisor};
    use crate::test_support::MockSandbox;
    use crate::tools::WEB_SEARCH_TOOL_NAME;

    fn native_tool_options(
        profile_kind: AgentProfileKind,
        has_web_search: bool,
    ) -> NativeToolOptions {
        let mut options = NativeToolOptions::for_profile(profile_kind);
        options.secrets.brave_search_api_key = has_web_search.then(|| "configured-key".to_string());
        options
    }

    fn system_prompt(profile: &dyn AgentProfile) -> String {
        let env = MockSandbox::linux();
        let context = EnvContext::default();
        profile.build_system_prompt(&env, &context, &[], None, &[])
    }

    fn register_test_subagent_tools(profile: &mut dyn AgentProfile) {
        let factory: SessionFactory = Arc::new(|| {
            panic!("should not be called while rendering a system prompt");
        });
        profile.register_subagent_tools(SubAgentSupervisor::new(3), factory, 0);
    }

    fn anthropic_profile(has_web_search: bool, has_subagents: bool) -> AnthropicProfile {
        let options = native_tool_options(AgentProfileKind::Anthropic, has_web_search);
        let mut profile = AnthropicProfile::with_native_tools("claude-haiku-4-5", &options, None);
        if has_subagents {
            register_test_subagent_tools(&mut profile);
        }
        profile
    }

    fn gemini_profile(has_web_search: bool) -> GeminiProfile {
        let options = native_tool_options(AgentProfileKind::Gemini, has_web_search);
        GeminiProfile::with_native_tools("gemini-3-flash-preview", &options, None)
    }

    fn openai_apply_patch_profile(has_web_search: bool) -> OpenAiProfile {
        let options = native_tool_options(AgentProfileKind::OpenAi, has_web_search);
        OpenAiProfile::with_native_tools("gpt-5.4-mini", &options, None)
    }

    fn openai_edit_file_profile(has_web_search: bool) -> OpenAiProfile {
        let options = native_tool_options(AgentProfileKind::OpenAi, has_web_search);
        OpenAiProfile::with_native_tools("kimi-k2.5", &options, None)
            .with_provider_id(ProviderId::new("kimi"))
            .with_catalog(Arc::new(Catalog::from_builtin().unwrap()))
    }

    /// Every provider gets the one shared `shell` definition, so the Bash
    /// contract is stated identically to OpenAI, Anthropic, and Gemini rather
    /// than drifting per provider.
    #[test]
    fn every_profile_advertises_the_same_bash_shell_tool() {
        let profiles: [Box<dyn AgentProfile>; 3] = [
            Box::new(anthropic_profile(false, false)),
            Box::new(gemini_profile(false)),
            Box::new(openai_apply_patch_profile(false)),
        ];

        let definitions: Vec<ToolDefinition> = profiles
            .iter()
            .map(|profile| {
                profile
                    .tools()
                    .into_iter()
                    .find(|tool| tool.name == "shell")
                    .expect("every profile should register the shell tool")
            })
            .collect();

        for definition in &definitions {
            assert_eq!(definition.parameters, definitions[0].parameters);
            assert_eq!(definition.description, definitions[0].description);
            assert!(
                definition.description.contains("Bash"),
                "shell tool should identify Bash: {}",
                definition.description
            );
        }
    }

    #[test]
    fn env_context_block_contains_platform() {
        let env = MockSandbox::linux();
        let block = build_env_context_block(&env);
        assert!(block.contains("<environment>"));
        assert!(block.contains("</environment>"));
        assert!(block.contains("linux"));
        assert!(block.contains("/home/test"));
        assert!(block.contains("Linux 6.1.0"));
    }

    #[test]
    fn env_context_block_with_extra_context() {
        let env = MockSandbox::linux();
        let ctx = EnvContext {
            git_branch:         Some("main".into()),
            is_git_repo:        true,
            current_date:       "2026-02-20".into(),
            model:              "claude-opus-4-6".into(),
            knowledge_cutoff:   "May 2025".into(),
            git_status_short:   None,
            git_recent_commits: None,
        };
        let block = build_env_context_block_with(&env, &ctx);
        assert!(block.contains("Git branch: main"));
        assert!(block.contains("Is git repository: true"));
        assert!(block.contains("Today's date: 2026-02-20"));
        assert!(block.contains("Model: claude-opus-4-6"));
        assert!(block.contains("Knowledge cutoff: May 2025"));
    }

    #[test]
    fn profile_builder_keeps_tool_availability_and_prompt_guidance_in_sync() {
        let catalog = Arc::new(Catalog::from_builtin().unwrap());
        let env = MockSandbox::linux();
        let cases = [
            (
                AgentProfileKind::OpenAi,
                ProviderId::openai(),
                "gpt-5.4-mini",
            ),
            (
                AgentProfileKind::Anthropic,
                ProviderId::anthropic(),
                "claude-haiku-4-5",
            ),
            (
                AgentProfileKind::Gemini,
                ProviderId::gemini(),
                "gemini-3-flash-preview",
            ),
        ];

        for (profile_kind, provider_id, model) in cases {
            let profile = AgentProfileBuilder::new(
                profile_kind,
                provider_id.clone(),
                model,
                Arc::clone(&catalog),
            )
            .build();
            assert_eq!(profile.profile_kind(), profile_kind);
            assert_eq!(profile.provider_id(), provider_id);
            assert!(profile.tool_registry().get(WEB_SEARCH_TOOL_NAME).is_none());
            let prompt = profile.build_system_prompt(&env, &EnvContext::default(), &[], None, &[]);
            assert!(
                !prompt.contains("web_search"),
                "{profile_kind:?} prompt advertised an unavailable tool"
            );

            let configured_builder = AgentProfileBuilder::new(
                profile_kind,
                profile.provider_id(),
                model,
                Arc::clone(&catalog),
            )
            .with_tool_secrets(ToolSecrets {
                brave_search_api_key: Some("configured-key".to_string()),
            });
            // Built twice: one configured builder must outfit both a root
            // session and the child sessions it spawns.
            for configured in [configured_builder.build(), configured_builder.build()] {
                assert!(
                    configured
                        .tool_registry()
                        .get(WEB_SEARCH_TOOL_NAME)
                        .is_some()
                );
                let prompt =
                    configured.build_system_prompt(&env, &EnvContext::default(), &[], None, &[]);
                assert!(
                    prompt.contains("web_search"),
                    "{profile_kind:?} prompt omitted guidance for an available tool"
                );
            }
        }
    }

    #[test]
    fn anthropic_default_prompt_snapshot() {
        insta::assert_snapshot!(system_prompt(&anthropic_profile(false, false)));
    }

    #[test]
    fn anthropic_web_search_prompt_snapshot() {
        insta::assert_snapshot!(system_prompt(&anthropic_profile(true, false)));
    }

    #[test]
    fn anthropic_subagents_prompt_snapshot() {
        insta::assert_snapshot!(system_prompt(&anthropic_profile(false, true)));
    }

    #[test]
    fn anthropic_web_search_and_subagents_prompt_snapshot() {
        insta::assert_snapshot!(system_prompt(&anthropic_profile(true, true)));
    }

    #[test]
    fn gemini_default_prompt_snapshot() {
        insta::assert_snapshot!(system_prompt(&gemini_profile(false)));
    }

    #[test]
    fn gemini_web_search_prompt_snapshot() {
        insta::assert_snapshot!(system_prompt(&gemini_profile(true)));
    }

    #[test]
    fn openai_apply_patch_prompt_snapshot() {
        insta::assert_snapshot!(system_prompt(&openai_apply_patch_profile(false)));
    }

    #[test]
    fn openai_apply_patch_and_web_search_prompt_snapshot() {
        insta::assert_snapshot!(system_prompt(&openai_apply_patch_profile(true)));
    }

    #[test]
    fn openai_edit_file_prompt_snapshot() {
        insta::assert_snapshot!(system_prompt(&openai_edit_file_profile(false)));
    }

    #[test]
    fn openai_edit_file_and_web_search_prompt_snapshot() {
        insta::assert_snapshot!(system_prompt(&openai_edit_file_profile(true)));
    }
}
