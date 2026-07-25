use fabro_types::{AgentToolCategory, PermissionLevel};

use crate::native_tool::NativeTool;

/// Resolve a tool name in any profile's vocabulary to the canonical name the
/// rest of the system reasons about.
///
/// A profile may expose a built-in tool under the vocabulary its model was
/// trained against — the Kimi profile uses Kimi Code's `Read`/`Edit`/`Bash`
/// names — but permissions, categories, and telemetry must not depend on which
/// profile is running. Names that are not built-in (MCP, skill, run-scoped)
/// pass through unchanged.
#[must_use]
pub fn canonical_tool_name(name: &str) -> &str {
    match NativeTool::from_any_name(name) {
        Some(tool) => tool.canonical_name(),
        None => name,
    }
}

/// Coarse access category for an exposed tool. Returns `None` for names
/// outside the permission taxonomy so callers can decide what that means: the
/// CLI gate defaults them to `Shell`, projection metadata reports `Other`.
pub fn known_tool_category(name: &str) -> Option<AgentToolCategory> {
    NativeTool::from_any_name(name).and_then(NativeTool::category)
}

/// CLI permission gate category. Unknown tools fall back to `Shell` so they
/// require explicit user approval at any permission level below `Full`.
pub fn tool_category(name: &str) -> AgentToolCategory {
    known_tool_category(name).unwrap_or(AgentToolCategory::Shell)
}

pub fn is_auto_approved(level: PermissionLevel, category: AgentToolCategory) -> bool {
    matches!(
        (level, category),
        (_, AgentToolCategory::Read | AgentToolCategory::Subagent)
            | (
                PermissionLevel::ReadWrite | PermissionLevel::Full,
                AgentToolCategory::Write,
            )
            | (PermissionLevel::Full, AgentToolCategory::Shell)
    )
}

pub fn is_tool_auto_approved(level: PermissionLevel, tool_name: &str) -> bool {
    is_auto_approved(level, tool_category(tool_name))
}
