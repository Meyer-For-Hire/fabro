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
use crate::sandbox::GrepOptions;
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
    use crate::test_support::{MockSandbox, MutableMockSandbox};
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

    /// `files_with_matches` and `count` both need the file path, which the
    /// underlying search only prefixes when scanning a directory.
    #[test]
    fn grep_result_path_handles_both_output_shapes() {
        // Directory scan: `<path>:<line>:<content>`.
        assert_eq!(
            grep_result_path("src/main.rs:42:fn main() {", "src"),
            "src/main.rs"
        );
        // A colon in the content must not be mistaken for the line field.
        assert_eq!(
            grep_result_path("src/a.rs:7:let x: u8 = 1;", "src"),
            "src/a.rs"
        );
        // Single-file scan omits the path, so fall back to what was searched.
        assert_eq!(
            grep_result_path("42:fn main() {", "src/main.rs"),
            "src/main.rs"
        );
    }

    async fn grep_with(args: serde_json::Value, lines: Vec<String>) -> Result<String, String> {
        let env: Arc<dyn Sandbox> = Arc::new(MockSandbox {
            grep_results: lines,
            ..MockSandbox::default()
        });
        let tool = make_kimi_grep_tool();
        (tool.executor)(args, ctx(env)).await
    }

    #[tokio::test]
    async fn grep_content_mode_returns_matching_lines() {
        let out = grep_with(json!({"pattern": "x"}), vec![
            "a.rs:1:x".into(),
            "b.rs:2:x".into(),
        ])
        .await
        .unwrap();
        assert_eq!(out, "a.rs:1:x\nb.rs:2:x");
    }

    #[tokio::test]
    async fn grep_files_with_matches_deduplicates_paths_in_order() {
        let out = grep_with(
            json!({"pattern": "x", "output_mode": "files_with_matches"}),
            vec!["a.rs:1:x".into(), "a.rs:9:x".into(), "b.rs:2:x".into()],
        )
        .await
        .unwrap();
        assert_eq!(out, "a.rs\nb.rs");
    }

    #[tokio::test]
    async fn grep_count_mode_counts_per_file() {
        let out = grep_with(json!({"pattern": "x", "output_mode": "count"}), vec![
            "a.rs:1:x".into(),
            "a.rs:9:x".into(),
            "b.rs:2:x".into(),
        ])
        .await
        .unwrap();
        assert_eq!(out, "a.rs:2\nb.rs:1");
    }

    #[tokio::test]
    async fn grep_offset_and_head_limit_page_results() {
        let lines: Vec<String> = (1..=6).map(|n| format!("f{n}.rs:1:x")).collect();
        let out = grep_with(json!({"pattern": "x", "offset": 2, "head_limit": 2}), lines)
            .await
            .unwrap();
        assert_eq!(out, "f3.rs:1:x\nf4.rs:1:x");
    }

    #[tokio::test]
    async fn grep_rejects_an_unknown_output_mode() {
        let err = grep_with(json!({"pattern": "x", "output_mode": "json"}), vec![])
            .await
            .unwrap_err();
        assert!(
            err.contains("expected content|files_with_matches|count"),
            "{err}"
        );
    }

    #[tokio::test]
    async fn grep_reports_no_matches_plainly() {
        let out = grep_with(json!({"pattern": "x"}), vec![]).await.unwrap();
        assert_eq!(out, "No matches found");
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

/// Output shapes Kimi Code's `Grep` supports.
#[derive(Clone, Copy, PartialEq, Eq)]
enum GrepOutputMode {
    Content,
    FilesWithMatches,
    Count,
}

impl GrepOutputMode {
    fn parse(value: Option<&str>) -> Result<Self, String> {
        match value.unwrap_or("content") {
            "content" => Ok(Self::Content),
            "files_with_matches" => Ok(Self::FilesWithMatches),
            "count" => Ok(Self::Count),
            other => Err(format!(
                "Invalid output_mode `{other}` (expected content|files_with_matches|count)"
            )),
        }
    }
}

/// Extract the file path from a grep result line.
///
/// The underlying search emits `<path>:<line>:<content>` when scanning a
/// directory, but omits the path when scanning a single file, so fall back to
/// the path that was searched.
fn grep_result_path<'a>(line: &'a str, searched: &'a str) -> &'a str {
    // Walk candidate separators so absolute Windows-style paths and paths
    // containing colons still split at the line-number field.
    let mut rest = line;
    let mut consumed = 0usize;
    while let Some(idx) = rest.find(':') {
        let after = &rest[idx + 1..];
        let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
        if !digits.is_empty() && after[digits.len()..].starts_with(':') {
            return &line[..consumed + idx];
        }
        consumed += idx + 1;
        rest = after;
    }
    searched
}

/// `Grep` with Kimi Code's `output_mode`, `head_limit`, and `offset`.
///
/// These are all shapes of the result list the sandbox already returns, so no
/// provider work is needed. Kimi Code's `type`, `multiline`, and
/// `include_ignored` are deliberately absent: they would have to reach ripgrep
/// flags through new `Sandbox` trait methods, and advertising a parameter that
/// is ignored is worse than omitting it.
#[must_use]
pub fn make_kimi_grep_tool() -> RegisteredTool {
    RegisteredTool {
        definition: definition(
            NativeTool::Grep,
            "Search file contents with a regular expression.

Use Grep when looking for unknown content or an unknown location. If you already know the path, \
use Read instead. Prefer this over running `grep` or `rg` through Bash: it caps its output, so it \
will not flood the conversation.

- Backed by ripgrep when available and POSIX `grep` otherwise, so keep patterns portable across \
both rather than relying on ripgrep-only syntax.
- `output_mode` selects what comes back: `content` (matching lines, the default), \
`files_with_matches` (just the paths), or `count` (matches per file).
- `head_limit` caps how many results are returned and `offset` skips that many first, so you can \
page through a large result set.
- `glob` limits which files are searched; `case_insensitive` folds case.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": {"type": "string", "description": "Regular expression to search for."},
                    "path": {"type": "string", "description": "Directory or file to search. Defaults to the working directory."},
                    "glob": {"type": "string", "description": "Only search files matching this glob."},
                    "output_mode": {
                        "type": "string",
                        "enum": ["content", "files_with_matches", "count"],
                        "description": "Shape of the results (default content)."
                    },
                    "head_limit": {"type": "integer", "description": "Return at most this many results."},
                    "offset": {"type": "integer", "description": "Skip this many results before returning."},
                    "case_insensitive": {"type": "boolean", "description": "Fold case when matching."}
                },
                "required": ["pattern"]
            }),
        ),
        executor:   Arc::new(|args, ctx| {
            Box::pin(async move {
                let pattern = required_str(&args, "pattern")?;
                // The trait requires a search root; "." is the working directory.
                let path = args.get("path").and_then(Value::as_str).unwrap_or(".");
                let mode = GrepOutputMode::parse(args.get("output_mode").and_then(Value::as_str))?;
                let head_limit = optional_usize_arg(&args, "head_limit")?;
                let offset = optional_usize_arg(&args, "offset")?.unwrap_or(0);

                let options = GrepOptions {
                    glob_filter:      args.get("glob").and_then(Value::as_str).map(str::to_string),
                    case_insensitive: args
                        .get("case_insensitive")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    // Only push the cap down for `content`, where results and
                    // lines are the same thing. Capping lines early would
                    // undercount files for the other modes.
                    max_results:      match mode {
                        GrepOutputMode::Content => head_limit.map(|n| n.saturating_add(offset)),
                        _ => None,
                    },
                };

                let lines = ctx
                    .env
                    .grep(pattern, path, &options)
                    .await
                    .map_err(|e| e.display_with_causes())?;

                let searched = path;
                let mut results: Vec<String> = match mode {
                    GrepOutputMode::Content => lines,
                    GrepOutputMode::FilesWithMatches => {
                        let mut seen: Vec<String> = Vec::new();
                        for line in &lines {
                            let file = grep_result_path(line, searched).to_string();
                            if !seen.contains(&file) {
                                seen.push(file);
                            }
                        }
                        seen
                    }
                    GrepOutputMode::Count => {
                        let mut counts: Vec<(String, usize)> = Vec::new();
                        for line in &lines {
                            let file = grep_result_path(line, searched).to_string();
                            match counts.iter_mut().find(|(name, _)| *name == file) {
                                Some((_, count)) => *count += 1,
                                None => counts.push((file, 1)),
                            }
                        }
                        counts
                            .into_iter()
                            .map(|(file, count)| format!("{file}:{count}"))
                            .collect()
                    }
                };

                if offset > 0 {
                    results = results.into_iter().skip(offset).collect();
                }
                if let Some(limit) = head_limit {
                    results.truncate(limit);
                }

                if results.is_empty() {
                    return Ok("No matches found".to_string());
                }
                Ok(results.join("\n"))
            })
        }),
        source:     ToolSource::Native,
    }
}
