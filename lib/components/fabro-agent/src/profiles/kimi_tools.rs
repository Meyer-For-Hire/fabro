//! Tools whose behavior differs from fabro's built-ins, implemented to Kimi
//! Code's contract.
//!
//! Where a Kimi Code tool behaves identically to an existing fabro tool, the
//! Kimi profile reuses that tool and only its exposed name changes (see
//! [`crate::native_tool::ToolVocabulary`]). These three differ in what their
//! parameters *mean*, not just what they are called, so renaming fabro's
//! parameters would advertise behavior fabro does not have:
//!
//! - `Bash` takes `timeout` in **seconds** where fabro takes milliseconds, and
//!   accepts a `cwd`. A rename alone would make every timeout 1000x wrong.
//! - `Read` accepts a **negative** `line_offset`, meaning "read the last N
//!   lines". Fabro's `offset` has no such meaning.
//! - `Write` takes a `mode`, so it can append. Fabro's write always replaces.
//!
//! Everything these tools do reaches the environment through the same
//! [`Sandbox`](crate::sandbox::Sandbox) methods the built-ins use, so sandbox
//! behavior, path policy, and the read-before-write guard are unchanged.

use std::fmt::Write as _;
use std::sync::Arc;

use fabro_llm::types::ToolDefinition;
use serde_json::Value;

use crate::native_tool::NativeTool;
use crate::tool_registry::{RegisteredTool, ToolSource};
use crate::tools::{optional_usize_arg, required_str};

/// Largest `n_lines` a single `Read` call returns, matching fabro's built-in
/// read default so the two tools cannot disagree about how much is "a page".
const DEFAULT_READ_LINES: usize = 2000;

fn definition(tool: NativeTool, description: &str, parameters: Value) -> ToolDefinition {
    ToolDefinition {
        // Registered under the canonical name; the registry's vocabulary
        // renames it on the way in.
        name: tool.canonical_name().to_string(),
        description: description.to_string(),
        parameters,
    }
}

/// `Bash`, taking `timeout` in seconds and an optional `cwd`.
#[must_use]
pub fn make_kimi_bash_tool(default_timeout_ms: u64, max_timeout_ms: u64) -> RegisteredTool {
    let default_timeout_s = default_timeout_ms / 1000;
    let max_timeout_s = max_timeout_ms / 1000;
    let description = format!(
        "Execute a bash command. Use this for shell semantics — pipes, env, processes, git, \
package managers, build and test runners.

Translate these to a dedicated tool instead:
- `cat` / `head` / `tail` on a known path → Read
- `sed` / `awk` for an in-place edit → Edit
- `echo > file` / heredoc → Write
- `find` or recursive `ls` to locate files by name → Glob (plain `ls <dir>` is fine)
- `grep` / `rg` to search file contents → Grep

The dedicated tools cap their output, so they keep large raw dumps out of the conversation.

Output: stdout and stderr are combined and returned as a string. A non-zero exit appends a \
`Command failed with exit code: N` line.

Guidelines:
- Each call runs in a fresh bash process. Environment variables and `cd` do NOT persist between \
calls — pass `cwd`, or use absolute paths.
- `timeout` is in SECONDS. It defaults to {default_timeout_s} and is capped at {max_timeout_s}.
- A long-running command needs a raised `timeout`, not a retry: a command that timed out once \
will time out again.
- Do not run interactive commands, or commands that never exit.
- Chain genuinely dependent steps with `&&`. Issue independent read-only commands as separate \
parallel calls in one response so their output stays separate.
- Quote paths containing spaces.
- Avoid `..` to reach outside the working directory, and do not modify files outside it unless \
explicitly asked. Never run commands requiring superuser privileges unless explicitly asked."
    );

    RegisteredTool {
        definition: definition(
            NativeTool::Shell,
            &description,
            serde_json::json!({
                "type": "object",
                "properties": {
                    "command": {"type": "string", "description": "The command to execute."},
                    "cwd": {
                        "type": "string",
                        "description": "Directory to run the command in. Defaults to the \
            working directory."
                    },
                    "timeout": {
                        "type": "integer",
                        "description": format!(
                            "Timeout in seconds (default {default_timeout_s}, max {max_timeout_s})."
                        )
                    },
                    "description": {
                        "type": "string",
                        "description": "Short description of what this command does."
                    }
                },
                "required": ["command"]
            }),
        ),
        executor:   Arc::new(move |args, ctx| {
            Box::pin(async move {
                let command = required_str(&args, "command")?;
                let cwd = args.get("cwd").and_then(Value::as_str);
                // Seconds on the wire, milliseconds in the sandbox.
                let timeout_ms = match args.get("timeout").and_then(Value::as_u64) {
                    Some(seconds) => seconds.saturating_mul(1000).min(max_timeout_ms),
                    None => default_timeout_ms,
                };

                let result = ctx
                    .env
                    .exec_command(command, timeout_ms, cwd, None, Some(ctx.cancel.clone()))
                    .await
                    .map_err(|e| e.display_with_causes())?;

                let mut out = result.stdout;
                if !result.stderr.is_empty() {
                    if !out.is_empty() {
                        out.push('\n');
                    }
                    out.push_str(&result.stderr);
                }
                if let Some(code) = result.exit_code.filter(|c| *c != 0) {
                    if !out.is_empty() {
                        out.push('\n');
                    }
                    let _ = write!(out, "Command failed with exit code: {code}");
                }
                Ok(out)
            })
        }),
        source:     ToolSource::Native,
    }
}

/// `Read`, where a negative `line_offset` reads from the end of the file.
#[must_use]
pub fn make_kimi_read_tool() -> RegisteredTool {
    RegisteredTool {
        definition: definition(
            NativeTool::ReadFile,
            "Read a text file from the workspace.

Reading a file is also what clears it for writing: Edit and Write refuse a file that has not been \
read in this session.

- If you have a concrete path, call Read directly. Do not Glob or `ls` first to check that it \
exists — a missing path returns an error you can handle.
- When you need several files, emit multiple Read calls in one response rather than one per turn.
- Returns `<line-number>\\t<content>` per line. Drop the number and tab when taking text for an \
Edit `old_string`.
- `line_offset` is the 1-based first line to read. A NEGATIVE value reads from the end, so -100 \
returns the last 100 lines.
- `n_lines` defaults to 2000 lines.
- Use Bash or an MCP tool for binary formats; this tool reads text.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Path to the file to read."},
                    "line_offset": {
                        "type": "integer",
                        "description": "1-based first line to read. Negative reads from the end \
            of the file (-100 reads the last 100 lines)."
                    },
                    "n_lines": {
                        "type": "integer",
                        "description": "Number of lines to read (default 2000)."
                    }
                },
                "required": ["path"]
            }),
        ),
        executor:   Arc::new(|args, ctx| {
            Box::pin(async move {
                let path = required_str(&args, "path")?;
                let n_lines = optional_usize_arg(&args, "n_lines")?;
                let line_offset = args.get("line_offset").and_then(Value::as_i64);

                let content = match line_offset {
                    // Negative offset: count the file's lines, then start that
                    // many from the end. Kimi Code's semantics.
                    Some(offset) if offset < 0 => {
                        let from_end = usize::try_from(offset.unsigned_abs())
                            .map_err(|_| "line_offset is too large".to_string())?;
                        let total = ctx
                            .env
                            .read_file(path, None, None)
                            .await
                            .map_err(|e| e.display_with_causes())?
                            .lines()
                            .count();
                        let start = total.saturating_sub(from_end).saturating_add(1);
                        ctx.env
                            .read_file(path, Some(start), n_lines.or(Some(from_end)))
                            .await
                    }
                    Some(offset) => {
                        let start = usize::try_from(offset)
                            .map_err(|_| "line_offset must fit in usize".to_string())?;
                        ctx.env.read_file(path, Some(start), n_lines).await
                    }
                    None => {
                        ctx.env
                            .read_file(path, None, n_lines.or(Some(DEFAULT_READ_LINES)))
                            .await
                    }
                }
                .map_err(|e| e.display_with_causes())?;

                ctx.env.mark_agent_read(path);
                Ok(content)
            })
        }),
        source:     ToolSource::Native,
    }
}

/// `Write`, with Kimi Code's `mode` so it can append.
#[must_use]
pub fn make_kimi_write_tool() -> RegisteredTool {
    RegisteredTool {
        definition: definition(
            NativeTool::WriteFile,
            "Create, append to, or replace a file.

Read an existing file with Read before writing to it — this workspace refuses writes to files \
that have not been read, and the call will fail.

- `mode` defaults to `overwrite`, which replaces the whole file. `append` adds to the end without \
inserting a newline.
- Write is NOT for incremental changes to an existing file, however small. Use Edit instead: \
overwrite replaces everything you did not restate.
- Use Write when the file does not exist, or when you intend a complete replacement.
- Do not create documentation files that were not asked for.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Path to the file to write."},
                    "content": {"type": "string", "description": "Content to write."},
                    "mode": {
                        "type": "string",
                        "enum": ["overwrite", "append"],
                        "description": "Whether to replace the file or append to it (default \
            overwrite)."
                    }
                },
                "required": ["path", "content"]
            }),
        ),
        executor:   Arc::new(|args, ctx| {
            Box::pin(async move {
                let path = required_str(&args, "path")?;
                let content = required_str(&args, "content")?;
                let mode = args
                    .get("mode")
                    .and_then(Value::as_str)
                    .unwrap_or("overwrite");

                let payload = match mode {
                    "overwrite" => content.to_string(),
                    // The sandbox trait has no append; read-modify-write keeps
                    // every provider working and stays inside path policy.
                    "append" => {
                        let existing = ctx.env.read_file_text(path).await.unwrap_or_default();
                        format!("{existing}{content}")
                    }
                    other => {
                        return Err(format!(
                            "Invalid mode `{other}` (expected overwrite|append)"
                        ));
                    }
                };

                ctx.env
                    .write_file(path, &payload)
                    .await
                    .map_err(|e| e.display_with_causes())?;
                Ok(format!("Wrote {path}"))
            })
        }),
        source:     ToolSource::Native,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use serde_json::json;
    use tokio_util::sync::CancellationToken;

    use super::*;
    use crate::sandbox::Sandbox;
    use crate::test_support::MutableMockSandbox;
    use crate::tool_registry::ToolContext;

    fn ctx(env: Arc<dyn Sandbox>) -> ToolContext {
        ToolContext {
            env,
            cancel: CancellationToken::new(),
            tool_env_provider: None,
            session_id: Some("ses".into()),
            root_session_id: Some("ses".into()),
            tool_call_id: None,
            agent_event_emitter: None,
        }
    }

    fn sandbox_with(path: &str, content: &str) -> Arc<MutableMockSandbox> {
        let mut files = HashMap::new();
        files.insert(path.to_string(), content.to_string());
        Arc::new(MutableMockSandbox::new(files))
    }

    /// The reason Read is a separate tool: a negative `line_offset` means
    /// "the last N lines", which fabro's `offset` has no notion of.
    #[tokio::test]
    async fn read_negative_line_offset_reads_from_the_end() {
        let lines: Vec<String> = (1..=20).map(|n| format!("line{n}")).collect();
        let env = sandbox_with("/f.txt", &lines.join("\n"));
        let tool = make_kimi_read_tool();

        let out = (tool.executor)(
            json!({"path": "/f.txt", "line_offset": -3}),
            ctx(env.clone()),
        )
        .await
        .unwrap();

        assert!(out.contains("line18"), "{out}");
        assert!(out.contains("line20"), "{out}");
        assert!(
            !out.contains("line1\n"),
            "should not include the head: {out}"
        );
    }

    #[tokio::test]
    async fn read_positive_line_offset_starts_there() {
        let lines: Vec<String> = (1..=20).map(|n| format!("line{n}")).collect();
        let env = sandbox_with("/f.txt", &lines.join("\n"));
        let tool = make_kimi_read_tool();

        let out = (tool.executor)(
            json!({"path": "/f.txt", "line_offset": 5, "n_lines": 2}),
            ctx(env),
        )
        .await
        .unwrap();
        assert!(out.contains("line5"), "{out}");
        assert!(!out.contains("line8"), "{out}");
    }

    /// The reason Write is a separate tool: it has a mode, so it can append.
    #[tokio::test]
    async fn write_append_mode_preserves_existing_content() {
        let env = sandbox_with("/f.txt", "first");
        let tool = make_kimi_write_tool();

        (tool.executor)(
            json!({"path": "/f.txt", "content": "-second", "mode": "append"}),
            ctx(env.clone()),
        )
        .await
        .unwrap();

        assert_eq!(env.read_file_text("/f.txt").await.unwrap(), "first-second");
    }

    #[tokio::test]
    async fn write_defaults_to_overwrite() {
        let env = sandbox_with("/f.txt", "first");
        let tool = make_kimi_write_tool();
        (tool.executor)(
            json!({"path": "/f.txt", "content": "only"}),
            ctx(env.clone()),
        )
        .await
        .unwrap();
        assert_eq!(env.read_file_text("/f.txt").await.unwrap(), "only");
    }

    #[tokio::test]
    async fn write_rejects_an_unknown_mode() {
        let env = sandbox_with("/f.txt", "x");
        let tool = make_kimi_write_tool();
        let err = (tool.executor)(
            json!({"path": "/f.txt", "content": "y", "mode": "prepend"}),
            ctx(env),
        )
        .await
        .unwrap_err();
        assert!(err.contains("expected overwrite|append"), "{err}");
    }

    /// The reason Bash is a separate tool: `timeout` is seconds, not
    /// milliseconds. A rename would have made every timeout 1000x wrong.
    #[test]
    fn bash_schema_states_seconds_and_quotes_real_limits() {
        let tool = make_kimi_bash_tool(60_000, 600_000);
        let params = &tool.definition.parameters;
        let timeout = params["properties"]["timeout"]["description"]
            .as_str()
            .unwrap();
        assert!(timeout.contains("seconds"), "{timeout}");
        assert!(timeout.contains("60"), "default should be 60s: {timeout}");
        assert!(timeout.contains("600"), "max should be 600s: {timeout}");
        assert!(params["properties"].get("cwd").is_some(), "cwd missing");
        assert!(
            tool.definition
                .description
                .contains("timeout` is in SECONDS")
        );
        // Fabro has no background shell, so none is promised.
        assert!(!tool.definition.description.contains("run_in_background"));
    }
}
