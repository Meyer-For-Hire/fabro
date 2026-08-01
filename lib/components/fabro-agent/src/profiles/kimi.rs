use std::sync::Arc;

use fabro_model::{AgentProfileKind, Catalog, ProviderId};

use super::EnvContext;
use crate::agent_profile::AgentProfile;
use crate::config::NativeToolOptions;
use crate::native_tool::{NativeTool, ToolVocabulary};
use crate::profiles::{
    self, BaseProfile, EmbeddedPrompt, ProfileDeps, impl_base_profile_accessors, kimi_tools,
};
use crate::sandbox::Sandbox;
use crate::skills::Skill;
use crate::todo_runtime::TodoRuntime;
use crate::todo_tools::make_todo_list_tool;
use crate::tool_registry::ToolRegistry;
use crate::tools::register_discovery_and_web_tools;

const CORE_PROMPT: &str = include_str!("prompts/kimi.md.j2");

/// Kimi models repeatedly reconstruct `old_string` from memory rather than from
/// a fresh read: across two observed K3 implementation stages, 32 of 35 tool
/// failures were edits against a file the model had not read, or `old_string`
/// values recalled from an earlier version. Kimi Code carries this guidance in
/// its tool descriptions and nowhere in its system prompt, so this profile does
/// the same — the rule lands in the description of the tool being called.
const EDIT_FILE_DESCRIPTION: &str = "Perform exact replacements in existing files.

- Edit is mandatory for every incremental change, especially small edits. DO NOT use Write or \
Bash `sed`.
- Read the target file before every Edit. DO NOT call Edit from memory, stale context, or a \
guessed `old_string`.
- Take `old_string` and `new_string` from the Read output view, dropping the line-number prefix \
and separator; match only file content.
- `old_string` must be unique unless `replace_all` is set. If it is ambiguous, add surrounding \
context. Use `replace_all` only when every occurrence should change — for example, renaming a \
symbol throughout the file.
- DO NOT issue consecutive Edit calls on the same file. A previous Edit can invalidate a later \
Edit's `old_string`, causing `old_string not found`. Read the file again before the next Edit.
- If an Edit fails with `old_string not found`, re-read the file and take the exact text from the \
fresh output rather than guessing again.
- Preserve existing indentation.";

const GLOB_DESCRIPTION: &str = "Find files by search-root-relative path using a glob pattern. \
Results are sorted lexicographically by relative path.

Use this instead of `find` or recursive `ls` through Bash. Prefer patterns with a literal anchor \
— an extension or a subdirectory — over bare wildcards.

Good patterns:
- `*.rs` — direct children of the search root
- `**/*.rs` — files at any depth below the search root
- `src/*.rs` — directly inside `src/`, not recursive
- `src/**/*.rs` — recursive walk under a subdirectory
- `src/[lm]ib.rs` — a bracket expression matches one character

Avoid recursing into dependency or build output (`node_modules/**`, `target/**`): those produce \
thousands of matches and waste context. Narrow to a specific subpath instead. Results are files, \
so to locate a directory, glob for something inside it. Patterns must use `/`, be relative, and \
cannot contain a `..` segment.";

pub struct KimiProfile {
    base: BaseProfile,
}

impl KimiProfile {
    #[must_use]
    pub fn new(model: impl Into<String>) -> Self {
        let deps = ProfileDeps::standalone(NativeToolOptions::for_profile(AgentProfileKind::Kimi));
        Self::with_native_tools(model, &deps)
    }

    pub(crate) fn with_native_tools(model: impl Into<String>, deps: &ProfileDeps) -> Self {
        let options = &deps.options;
        // The registry carries the vocabulary, so tools registered later
        // (subagent tools, skills) are renamed too.
        let mut registry = ToolRegistry::with_vocabulary(ToolVocabulary::KimiCode);

        // Glob and the web tools have the same contract in both vocabularies.
        // The remaining Kimi tools use adapters for their different schemas,
        // while reusing shared execution helpers where their behavior agrees.
        register_discovery_and_web_tools(&mut registry, options, deps.summarizer.clone());
        registry.register(kimi_tools::make_kimi_read_tool());
        registry.register(kimi_tools::make_kimi_write_tool());
        registry.register(kimi_tools::make_kimi_edit_tool(EDIT_FILE_DESCRIPTION));
        registry.register(kimi_tools::make_kimi_grep_tool());
        registry.register(kimi_tools::make_kimi_bash_tool(
            options.default_command_timeout_ms,
            options.max_command_timeout_ms,
        ));
        registry.redescribe(NativeTool::Glob, GLOB_DESCRIPTION);

        // Kimi Code drives todos with one replace-whole-list call. The
        // Anthropic task tools model the opposite interaction -- incremental
        // mutation against tracked ids -- so they are the wrong surface here
        // even though both persist through the same runtime.
        let todo_runtime = Arc::new(TodoRuntime::new());
        registry.register(make_todo_list_tool(todo_runtime));

        Self {
            base: BaseProfile {
                profile_kind: AgentProfileKind::Kimi,
                provider_id: ProviderId::new("moonshot"),
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
    impl_base_profile_accessors!();

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
    use crate::skills::make_use_skill_tool_for_vocabulary;
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
            (catalog(), "moonshot", "kimi-k3"),
            (catalog(), "moonshot", "kimi-k2.5"),
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
        // Deliberately not a GPT-5.6 model: those carry their own per-model
        // profile override, so they would not show that the provider default
        // is what applies here.
        let profile =
            catalog.effective_agent_profile(&ProviderId::new("openrouter"), Some("gpt-5.4"));
        assert_eq!(profile, Some(AgentProfileKind::OpenAi));
    }

    /// The rename must not change what a tool is allowed to do. An exposed
    /// name that fails to resolve would fall back to `Shell` in the CLI gate,
    /// silently demanding approval for reads.
    #[test]
    fn renamed_tools_keep_their_permission_category() {
        let profile = KimiProfile::new("kimi-k3");
        for name in profile.tool_registry().names() {
            let tool = NativeTool::from_any_name(&name)
                .unwrap_or_else(|| panic!("unexpected non-native Kimi profile tool: {name}"));
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
            .register(make_use_skill_tool_for_vocabulary(
                Arc::new(vec![Skill {
                    name:        "demo".into(),
                    description: "d".into(),
                    template:    "t".into(),
                }]),
                ToolVocabulary::KimiCode,
            ));

        let names = profile.tool_registry().names();
        assert!(names.contains(&"Skill".to_string()), "got {names:?}");
        assert!(!names.contains(&"use_skill".to_string()), "got {names:?}");
        let skill_parameters = &profile
            .tool_registry()
            .get("Skill")
            .unwrap()
            .definition
            .parameters;
        assert!(skill_parameters["properties"].get("skill").is_some());
        assert!(skill_parameters["properties"].get("args").is_some());
        assert!(skill_parameters["properties"].get("skill_name").is_none());
        // Deliberately not renamed to Kimi Code's `Agent`: fabro's subagent
        // tools are a supervisor model, not a call-and-return one.
        assert!(names.contains(&"spawn_agent".to_string()), "got {names:?}");
    }

    #[test]
    fn edit_and_write_descriptions_drill_reading_first() {
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

        // Kimi Code carries read-before-edit guidance in the tool descriptions
        // and nowhere in its system prompt, so this is where it must land.
        for name in ["Edit", "Write"] {
            let text = describe(name);
            assert!(
                text.contains("Read"),
                "{name} should steer the model to read the file first"
            );
        }
        assert!(
            describe("Edit").contains("DO NOT call Edit from memory, stale context, or a guessed")
        );
        assert!(describe("Edit").contains("DO NOT issue consecutive Edit calls on the same file"));
        assert!(describe("Write").contains("Read before overwriting an existing file"));
        // Re-reading only to confirm a write landed is waste, not diligence.
        assert!(describe("Read").contains("do not re-read solely to prove the write landed"));

        // Bash steers shell usage toward the dedicated tools, under the names
        // this profile actually exposes.
        let bash = describe("Bash");
        for expected in ["→ Read", "→ Edit", "→ Write", "→ Glob", "→ Grep"] {
            assert!(bash.contains(expected), "Bash should map {expected}");
        }
        // Bash takes SECONDS, unlike fabro's millisecond built-in. Assert the
        // seconds value is quoted and the raw millisecond value is not, which
        // is what a unit bug would look like.
        let options = NativeToolOptions::for_profile(AgentProfileKind::Kimi);
        let seconds = (options.default_command_timeout_ms / 1000).to_string();
        assert!(
            bash.contains(&seconds),
            "Bash should quote {seconds}s: {bash}"
        );
        assert!(
            !bash.contains(&options.default_command_timeout_ms.to_string()),
            "Bash quotes milliseconds, so the unit conversion is wrong: {bash}"
        );
        assert!(bash.contains("SECONDS"), "{bash}");
        // Fabro has no background shell; promising one would be a lie.
        assert!(!bash.contains("run_in_background"), "{bash}");

        // Read tells the model how to turn its output into an Edit old_string.
        assert!(describe("Read").contains("Drop the number and separator"));
        // Grep must not promise ripgrep syntax: fabro falls back to POSIX grep.
        let grep = describe("Grep");
        assert!(grep.contains("POSIX"), "{grep}");
        assert!(describe("Glob").contains("sorted lexicographically"));
        assert!(describe("Glob").contains("`*.rs` — direct children"));
    }

    #[test]
    fn kimi_edit_schema_uses_path_like_kimi_code() {
        let profile = KimiProfile::new("kimi-k3");
        let parameters = &profile
            .tool_registry()
            .get("Edit")
            .unwrap()
            .definition
            .parameters;

        assert!(parameters["properties"].get("path").is_some());
        assert!(parameters["properties"].get("file_path").is_none());
        assert_eq!(
            parameters["required"],
            serde_json::json!(["path", "old_string", "new_string"])
        );
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
        assert!(prompt.contains("# Tracking Multi-Step Work"));
        assert!(prompt.contains("<environment>"));
        // Kimi Code keeps read-before-edit mechanics out of its system prompt
        // and in the tool descriptions; the profile follows that split.
        assert!(!prompt.contains("Reading Before Writing"));
    }
}
