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
//! fabro's own names by default, Kimi Code's names for the Kimi profile.
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
    /// The names Kimi Code exposes, for models trained against that harness.
    KimiCode,
}

/// A tool fabro implements itself.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Display, EnumString, IntoStaticStr, VariantArray,
)]
pub enum NativeTool {
    #[strum(to_string = "read_file")]
    ReadFile,
    #[strum(to_string = "read_many_files")]
    ReadManyFiles,
    #[strum(to_string = "write_file")]
    WriteFile,
    #[strum(to_string = "edit_file")]
    EditFile,
    #[strum(to_string = "apply_patch")]
    ApplyPatch,
    #[strum(to_string = "list_dir")]
    ListDir,
    #[strum(to_string = "grep")]
    Grep,
    #[strum(to_string = "glob")]
    Glob,
    #[strum(to_string = "shell")]
    Shell,
    #[strum(to_string = "web_search")]
    WebSearch,
    #[strum(to_string = "web_fetch")]
    WebFetch,
    #[strum(to_string = "spawn_agent")]
    SpawnAgent,
    #[strum(to_string = "send_input")]
    SendInput,
    #[strum(to_string = "wait")]
    Wait,
    #[strum(to_string = "close_agent")]
    CloseAgent,
    #[strum(to_string = "use_skill")]
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

    /// The name this tool is exposed under in `vocabulary`.
    ///
    /// A tool with no counterpart in the vocabulary keeps its canonical name.
    #[must_use]
    pub fn name(self, vocabulary: ToolVocabulary) -> &'static str {
        match vocabulary {
            ToolVocabulary::Fabro => self.canonical_name(),
            ToolVocabulary::KimiCode => match self {
                Self::ReadFile => "Read",
                Self::WriteFile => "Write",
                Self::EditFile => "Edit",
                Self::Shell => "Bash",
                Self::Grep => "Grep",
                Self::Glob => "Glob",
                Self::WebSearch => "WebSearch",
                Self::WebFetch => "FetchURL",
                // Kimi Code has no counterpart with these semantics: its
                // TodoList replaces a whole list rather than mutating tasks,
                // and it has no equivalent of the remaining tools.
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
        Self::VARIANTS.iter().copied().find(|tool| {
            ToolVocabulary::VARIANTS
                .iter()
                .any(|vocabulary| tool.name(*vocabulary) == name)
        })
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
            Self::SpawnAgent | Self::SendInput | Self::Wait | Self::CloseAgent => {
                Some(AgentToolCategory::Subagent)
            }
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
