//! The built-in tools fabro implements, and the names they can be expressed
//! under.
//!
//! Tool names reach this crate from two very different places. The tools fabro
//! implements are a fixed set known at compile time; MCP, skill, and
//! run-scoped tools are open-ended and named by whatever registered them. This
//! module covers the first group, so anything reasoning about a built-in tool
//! is checked by the compiler instead of matched on string literals.
//!
//! A [`NativeTool`] is an identity, not a name. The same tool is expressed
//! under different names depending on the [`ToolVocabulary`] a profile speaks:
//! fabro's own names by default, Anthropic's names for Claude 5, Kimi Code's
//! names for the Kimi profile, and Codex's names for the GPT-5.6 profile.
//! Permissions, categories, and telemetry resolve any name back to the
//! identity, so behavior never depends on which vocabulary is in play.
//!
//! `ToolDefinition.name` and [`crate::tool_registry::ToolRegistry`] keys stay
//! `String`, because they carry both groups.

use fabro_types::AgentToolCategory;
use strum::{Display, EnumString, IntoStaticStr, VariantArray};

/// A naming scheme for built-in tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, VariantArray)]
pub enum ToolVocabulary {
    /// Fabro's own names, and the canonical identity used internally.
    #[default]
    Fabro,
    /// The names Anthropic's Claude 5 coding harness exposes.
    Claude5,
    /// The names Kimi Code exposes, for models trained against that harness.
    KimiCode,
    /// The names Codex exposes, for the GPT-5.6 models trained against it.
    Codex,
}

/// A tool fabro implements itself.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Display, EnumString, IntoStaticStr, VariantArray,
)]
pub enum NativeTool {
    #[strum(to_string = "read_file", serialize = "Read")]
    ReadFile,
    #[strum(to_string = "read_many_files")]
    ReadManyFiles,
    #[strum(to_string = "write_file", serialize = "Write")]
    WriteFile,
    #[strum(to_string = "edit_file", serialize = "Edit")]
    EditFile,
    #[strum(to_string = "apply_patch")]
    ApplyPatch,
    #[strum(to_string = "list_dir")]
    ListDir,
    #[strum(to_string = "grep", serialize = "Grep")]
    Grep,
    #[strum(to_string = "glob", serialize = "Glob")]
    Glob,
    #[strum(to_string = "shell", serialize = "Bash", serialize = "shell_command")]
    Shell,
    #[strum(to_string = "web_search", serialize = "WebSearch")]
    WebSearch,
    #[strum(
        to_string = "web_fetch",
        serialize = "FetchURL",
        serialize = "WebFetch"
    )]
    WebFetch,
    #[strum(to_string = "spawn_agent")]
    SpawnAgent,
    #[strum(to_string = "send_input")]
    SendInput,
    #[strum(to_string = "wait")]
    Wait,
    #[strum(to_string = "close_agent")]
    CloseAgent,
    // Claude 5 drives one background agent through four tools, where fabro's
    // own vocabulary uses `spawn_agent`/`wait`/`close_agent`/`send_input`.
    // They are separate identities rather than aliases of those because the
    // capabilities differ: `Agent` runs in the background or inline depending
    // on `run_in_background`, and `TaskOutput` both polls and waits. Mapping
    // them onto the fabro four would promise semantics those tools do not
    // have -- the same reason Kimi Code's `Agent` is deliberately unmapped.
    #[strum(to_string = "background_agent", serialize = "Agent")]
    BackgroundAgent,
    #[strum(to_string = "agent_output", serialize = "TaskOutput")]
    AgentOutput,
    #[strum(to_string = "stop_agent", serialize = "TaskStop")]
    StopAgent,
    #[strum(to_string = "message_agent", serialize = "SendMessage")]
    MessageAgent,
    #[strum(to_string = "use_skill", serialize = "Skill")]
    UseSkill,
    #[strum(to_string = "update_plan")]
    UpdatePlan,
    // Task and question tools are already PascalCase on the wire; they came
    // from the Claude Code vocabulary rather than fabro's own.
    #[strum(to_string = "TaskCreate")]
    TaskCreate,
    #[strum(to_string = "TaskUpdate")]
    TaskUpdate,
    #[strum(to_string = "TaskGet")]
    TaskGet,
    #[strum(to_string = "TaskList")]
    TaskList,
    #[strum(to_string = "TodoList")]
    TodoList,
    #[strum(to_string = "AskUserQuestion")]
    AskUserQuestion,
    #[strum(to_string = "request_user_input")]
    RequestUserInput,
}

impl NativeTool {
    /// The canonical name: how fabro refers to this tool internally.
    #[must_use]
    pub fn canonical_name(self) -> &'static str {
        self.into()
    }

    /// Resolve a canonical fabro name to its built-in identity.
    ///
    /// Unlike [`Self::from_any_name`], this deliberately ignores provider
    /// aliases. Registries use it while registering tools so an unrelated
    /// extension named `Read` is not silently treated as fabro's file reader.
    #[must_use]
    pub fn from_canonical_name(name: &str) -> Option<Self> {
        Self::VARIANTS
            .iter()
            .copied()
            .find(|tool| tool.canonical_name() == name)
    }

    /// The name this tool is exposed under in `vocabulary`.
    ///
    /// A tool with no counterpart in the vocabulary keeps its canonical name.
    #[must_use]
    pub fn name(self, vocabulary: ToolVocabulary) -> &'static str {
        match vocabulary {
            ToolVocabulary::Fabro => self.canonical_name(),
            ToolVocabulary::Claude5 => match self {
                Self::ReadFile => "Read",
                Self::WriteFile => "Write",
                Self::EditFile => "Edit",
                Self::Shell => "Bash",
                // Named for completeness: this arm describes the vocabulary,
                // not the profile's registry, and the Claude 5 profile
                // deliberately registers neither.
                Self::Grep => "Grep",
                Self::Glob => "Glob",
                Self::WebSearch => "WebSearch",
                Self::WebFetch => "WebFetch",
                Self::UseSkill => "Skill",
                Self::BackgroundAgent => "Agent",
                Self::AgentOutput => "TaskOutput",
                Self::StopAgent => "TaskStop",
                Self::MessageAgent => "SendMessage",
                other => other.canonical_name(),
            },
            ToolVocabulary::KimiCode => match self {
                Self::ReadFile => "Read",
                Self::WriteFile => "Write",
                Self::EditFile => "Edit",
                Self::Shell => "Bash",
                Self::Grep => "Grep",
                Self::Glob => "Glob",
                Self::WebSearch => "WebSearch",
                Self::WebFetch => "FetchURL",
                Self::UseSkill => "Skill",
                // Deliberately unmapped. Kimi Code's `Agent` launches a
                // subagent and returns its result; fabro's spawn_agent returns
                // a handle that send_input, wait, and close_agent then drive.
                // Borrowing the name without the semantics would promise a
                // result the tool does not return -- the same mistake as
                // exposing incremental task tools under a whole-list name.
                Self::SpawnAgent | Self::SendInput | Self::Wait | Self::CloseAgent => {
                    self.canonical_name()
                }
                other => other.canonical_name(),
            },
            // Codex names its shell `shell_command`. Its remaining tools that
            // fabro also implements -- apply_patch, update_plan,
            // request_user_input -- already agree with fabro's names, and the
            // tools fabro has that Codex does not keep fabro's names.
            //
            // Deliberately unmapped: Codex's sub-agent tools differ by
            // multi-agent protocol version rather than by name alone
            // (`resume_agent` has no fabro counterpart), and its `web.run` is a
            // namespaced tool, which fabro's registry cannot express.
            ToolVocabulary::Codex => match self {
                Self::Shell => "shell_command",
                other => other.canonical_name(),
            },
        }
    }

    /// Resolve a name in any known vocabulary back to the tool it identifies.
    ///
    /// Returns `None` for MCP, skill, and run-scoped tools, whose names are
    /// not drawn from this set.
    #[must_use]
    pub fn from_any_name(name: &str) -> Option<Self> {
        name.parse().ok()
    }

    /// Coarse access category, or `None` when the tool is not part of the
    /// permission taxonomy.
    ///
    /// Matched exhaustively so a new built-in tool has to state its answer.
    /// `None` is a real answer, and callers disagree about what it means: the
    /// CLI gate treats an uncategorized tool as `Shell` (requiring approval),
    /// while projection metadata reports `Other`.
    #[must_use]
    pub fn category(self) -> Option<AgentToolCategory> {
        match self {
            Self::ReadFile | Self::ReadManyFiles | Self::Grep | Self::Glob | Self::ListDir => {
                Some(AgentToolCategory::Read)
            }
            Self::WriteFile | Self::EditFile | Self::ApplyPatch => Some(AgentToolCategory::Write),
            Self::Shell => Some(AgentToolCategory::Shell),
            Self::SpawnAgent
            | Self::SendInput
            | Self::Wait
            | Self::CloseAgent
            | Self::BackgroundAgent
            | Self::AgentOutput
            | Self::StopAgent
            | Self::MessageAgent => Some(AgentToolCategory::Subagent),
            // Uncategorized today. Giving these a category would change the CLI
            // permission gate, which is a behavior change rather than a
            // classification cleanup, so they keep their existing answer.
            Self::WebSearch
            | Self::WebFetch
            | Self::UseSkill
            | Self::UpdatePlan
            | Self::TaskCreate
            | Self::TaskUpdate
            | Self::TaskGet
            | Self::TaskList
            | Self::TodoList
            | Self::AskUserQuestion
            | Self::RequestUserInput => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    #[test]
    fn canonical_names_round_trip() {
        for tool in NativeTool::VARIANTS {
            assert_eq!(NativeTool::from_str(tool.canonical_name()).unwrap(), *tool);
        }
    }

    #[test]
    fn every_name_in_every_vocabulary_resolves_back_to_its_tool() {
        for tool in NativeTool::VARIANTS {
            for vocabulary in ToolVocabulary::VARIANTS {
                let name = tool.name(*vocabulary);
                assert_eq!(
                    NativeTool::from_any_name(name),
                    Some(*tool),
                    "{name} ({vocabulary:?}) should resolve back to {tool}"
                );
            }
        }
    }

    /// Two tools resolving to the same name would make `from_any_name`
    /// ambiguous and silently mis-categorize one of them.
    #[test]
    fn vocabularies_do_not_collide() {
        let mut seen: Vec<(&str, NativeTool)> = Vec::new();
        for tool in NativeTool::VARIANTS {
            for vocabulary in ToolVocabulary::VARIANTS {
                let name = tool.name(*vocabulary);
                if let Some((_, other)) = seen.iter().find(|(seen, _)| *seen == name) {
                    assert_eq!(*other, *tool, "name '{name}' is claimed by two tools");
                } else {
                    seen.push((name, *tool));
                }
            }
        }
    }

    #[test]
    fn kimi_vocabulary_renames_only_where_kimi_code_differs() {
        assert_eq!(NativeTool::ReadFile.name(ToolVocabulary::KimiCode), "Read");
        assert_eq!(NativeTool::Shell.name(ToolVocabulary::KimiCode), "Bash");
        assert_eq!(
            NativeTool::WebFetch.name(ToolVocabulary::KimiCode),
            "FetchURL"
        );
        // No Kimi Code counterpart: keeps fabro's name.
        assert_eq!(
            NativeTool::TaskCreate.name(ToolVocabulary::KimiCode),
            "TaskCreate"
        );
        assert_eq!(
            NativeTool::SpawnAgent.name(ToolVocabulary::KimiCode),
            "spawn_agent"
        );
    }

    #[test]
    fn claude5_vocabulary_uses_anthropic_harness_names() {
        assert_eq!(NativeTool::ReadFile.name(ToolVocabulary::Claude5), "Read");
        assert_eq!(NativeTool::Shell.name(ToolVocabulary::Claude5), "Bash");
        assert_eq!(
            NativeTool::WebFetch.name(ToolVocabulary::Claude5),
            "WebFetch"
        );
        assert_eq!(
            NativeTool::BackgroundAgent.name(ToolVocabulary::Claude5),
            "Agent"
        );
        assert_eq!(
            NativeTool::AgentOutput.name(ToolVocabulary::Claude5),
            "TaskOutput"
        );
        assert_eq!(
            NativeTool::StopAgent.name(ToolVocabulary::Claude5),
            "TaskStop"
        );
        assert_eq!(
            NativeTool::MessageAgent.name(ToolVocabulary::Claude5),
            "SendMessage"
        );
    }

    /// The harness name is how a tool is expressed, not what it is: the
    /// identity keeps a fabro name, and the harness name resolves back to it.
    #[test]
    fn claude5_subagent_tools_keep_fabro_canonical_names() {
        for (tool, canonical, claude5) in [
            (NativeTool::BackgroundAgent, "background_agent", "Agent"),
            (NativeTool::AgentOutput, "agent_output", "TaskOutput"),
            (NativeTool::StopAgent, "stop_agent", "TaskStop"),
            (NativeTool::MessageAgent, "message_agent", "SendMessage"),
        ] {
            assert_eq!(tool.canonical_name(), canonical);
            assert_eq!(tool.name(ToolVocabulary::Fabro), canonical);
            assert_eq!(tool.name(ToolVocabulary::Claude5), claude5);
            assert_eq!(NativeTool::from_any_name(canonical), Some(tool));
            assert_eq!(NativeTool::from_any_name(claude5), Some(tool));
        }
    }

    #[test]
    fn codex_vocabulary_renames_only_the_shell() {
        assert_eq!(
            NativeTool::Shell.name(ToolVocabulary::Codex),
            "shell_command"
        );
        // Already agree with Codex's names.
        assert_eq!(
            NativeTool::ApplyPatch.name(ToolVocabulary::Codex),
            "apply_patch"
        );
        assert_eq!(
            NativeTool::UpdatePlan.name(ToolVocabulary::Codex),
            "update_plan"
        );
        assert_eq!(
            NativeTool::RequestUserInput.name(ToolVocabulary::Codex),
            "request_user_input"
        );
        // No Codex counterpart: keeps fabro's name.
        assert_eq!(
            NativeTool::ReadFile.name(ToolVocabulary::Codex),
            "read_file"
        );
    }

    /// The canonical name is what permissions, categories, and telemetry key
    /// on, so adding `shell_command` as a parse alias must not change it.
    #[test]
    fn shell_keeps_its_canonical_name_alongside_the_codex_alias() {
        assert_eq!(NativeTool::Shell.canonical_name(), "shell");
        assert_eq!(NativeTool::Shell.to_string(), "shell");
        assert_eq!(
            NativeTool::from_any_name("shell_command"),
            Some(NativeTool::Shell)
        );
        assert_eq!(NativeTool::from_canonical_name("shell_command"), None);
    }

    #[test]
    fn categories_are_vocabulary_independent() {
        for tool in NativeTool::VARIANTS {
            for vocabulary in ToolVocabulary::VARIANTS {
                let resolved = NativeTool::from_any_name(tool.name(*vocabulary))
                    .expect("known name should resolve");
                assert_eq!(resolved.category(), tool.category());
            }
        }
    }
}
