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
/// values are captured by tool executors during profile construction. Clone a
/// configured builder when root and child sessions must expose the same tools.
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
    pub fn with_command_timeouts(
        mut self,
        default_command_timeout_ms: u64,
        max_command_timeout_ms: u64,
    ) -> Self {
        self.native_tool_options.default_command_timeout_ms = default_command_timeout_ms;
        self.native_tool_options.max_command_timeout_ms = max_command_timeout_ms;
        self
    }

    #[must_use]
    pub fn with_web_fetch_summarizer(mut self, summarizer: Option<WebFetchSummarizer>) -> Self {
        self.summarizer = summarizer;
        self
    }

    #[must_use]
    pub fn build(self) -> Box<dyn AgentProfile> {
        match self.profile_kind {
            AgentProfileKind::OpenAi => Box::new(
                OpenAiProfile::with_native_tools(
                    self.model,
                    &self.native_tool_options,
                    self.summarizer,
                )
                .with_provider_id(self.provider_id)
                .with_catalog(self.catalog),
            ),
            AgentProfileKind::Gemini => Box::new(
                GeminiProfile::with_native_tools(
                    self.model,
                    &self.native_tool_options,
                    self.summarizer,
                )
                .with_provider_id(self.provider_id)
                .with_catalog(self.catalog),
            ),
            AgentProfileKind::Anthropic => Box::new(
                AnthropicProfile::with_native_tools(
                    self.model,
                    &self.native_tool_options,
                    self.summarizer,
                )
                .with_provider_id(self.provider_id)
                .with_catalog(self.catalog),
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

/// Assembles a complete system prompt from a core prompt template and standard
/// sections.
///
/// The `core_prompt` should contain `{env_block}` as a placeholder where the
/// environment context block will be inserted. Project docs and user
/// instructions are appended at the end.
#[must_use]
pub fn assemble_system_prompt(
    core_prompt: &str,
    env: &dyn Sandbox,
    env_context: &EnvContext,
    memory: &[String],
    user_instructions: Option<&str>,
    skills: &[Skill],
) -> String {
    let env_block = build_env_context_block_with(env, env_context);
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

    let prompt = core_prompt.replace("{env_block}", &env_block);
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
    use super::*;
    use crate::test_support::MockSandbox;

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
            assert!(profile.tool_registry().get("web_search").is_none());
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
            })
            .with_command_timeouts(20_000, 600_000);
            for configured in [
                configured_builder.clone().build(),
                configured_builder.build(),
            ] {
                assert!(configured.tool_registry().get("web_search").is_some());
                let prompt =
                    configured.build_system_prompt(&env, &EnvContext::default(), &[], None, &[]);
                assert!(
                    prompt.contains("web_search"),
                    "{profile_kind:?} prompt omitted guidance for an available tool"
                );
            }
        }
    }
}
