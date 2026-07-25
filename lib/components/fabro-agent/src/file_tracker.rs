use std::collections::BTreeMap;
use std::fmt::Write;

use fabro_llm::types::{ToolCall, ToolResult};

use crate::native_tool::NativeTool;
use crate::tool_permissions::canonical_tool_name;

fn file_path(arguments: &serde_json::Value) -> Option<&str> {
    arguments
        .get("file_path")
        .or_else(|| arguments.get("path"))
        .and_then(serde_json::Value::as_str)
}

#[derive(Debug, Clone, Copy, Default)]
struct FileOps {
    read:    bool,
    written: bool,
    edited:  bool,
}

#[derive(Debug, Default)]
pub struct FileTracker {
    files: BTreeMap<String, FileOps>,
}

impl FileTracker {
    pub fn record_read(&mut self, path: &str) {
        self.files.entry(path.to_string()).or_default().read = true;
    }

    pub fn record_write(&mut self, path: &str) {
        self.files.entry(path.to_string()).or_default().written = true;
    }

    pub fn record_edit(&mut self, path: &str) {
        self.files.entry(path.to_string()).or_default().edited = true;
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    pub fn render(&self) -> String {
        let mut output = String::new();
        for (path, ops) in &self.files {
            let mut labels = Vec::new();
            if ops.read {
                labels.push("read");
            }
            if ops.written {
                labels.push("written");
            }
            if ops.edited {
                labels.push("edited");
            }
            let _ = writeln!(output, "- {path} ({})", labels.join(", "));
        }
        output
    }

    pub fn record_from_tool_calls(&mut self, tool_calls: &[ToolCall], results: &[ToolResult]) {
        for (tc, result) in tool_calls.iter().zip(results.iter()) {
            if result.is_error {
                continue;
            }
            match canonical_tool_name(&tc.name) {
                name if name == NativeTool::ReadFile.canonical_name() => {
                    if let Some(path) = file_path(&tc.arguments) {
                        self.record_read(path);
                    }
                }
                name if name == NativeTool::WriteFile.canonical_name() => {
                    if let Some(path) = file_path(&tc.arguments) {
                        self.record_write(path);
                    }
                }
                name if name == NativeTool::EditFile.canonical_name() => {
                    if let Some(path) = file_path(&tc.arguments) {
                        self.record_edit(path);
                    }
                }
                name if name == NativeTool::ApplyPatch.canonical_name() => {
                    let content = match result.content.as_str() {
                        Some(s) => s.to_string(),
                        None => result.content.to_string(),
                    };
                    for line in content.lines() {
                        let line = line.trim();
                        if let Some(path) = line.strip_prefix("A ") {
                            self.record_write(path.trim());
                        } else if let Some(path) = line.strip_prefix("M ") {
                            self.record_edit(path.trim());
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_read_renders_read_flag() {
        let mut tracker = FileTracker::default();
        tracker.record_read("src/main.rs");
        assert_eq!(tracker.render(), "- src/main.rs (read)\n");
    }

    #[test]
    fn record_write_and_edit_renders_all_ops() {
        let mut tracker = FileTracker::default();
        tracker.record_read("src/lib.rs");
        tracker.record_write("src/lib.rs");
        tracker.record_edit("src/lib.rs");
        assert_eq!(tracker.render(), "- src/lib.rs (read, written, edited)\n");
    }

    #[test]
    fn multiple_files_sorted_by_path() {
        let mut tracker = FileTracker::default();
        tracker.record_write("z.rs");
        tracker.record_read("a.rs");
        let rendered = tracker.render();
        assert_eq!(rendered, "- a.rs (read)\n- z.rs (written)\n");
    }

    #[test]
    fn record_from_tool_calls_read_file() {
        let mut tracker = FileTracker::default();
        let tool_calls = vec![ToolCall::new(
            "tc1",
            "read_file",
            serde_json::json!({"file_path": "/tmp/foo.rs"}),
        )];
        let results = vec![ToolResult::success(
            "tc1",
            serde_json::json!("file contents"),
        )];
        tracker.record_from_tool_calls(&tool_calls, &results);
        assert_eq!(tracker.render(), "- /tmp/foo.rs (read)\n");
    }

    #[test]
    fn record_from_tool_calls_write_file() {
        let mut tracker = FileTracker::default();
        let tool_calls = vec![ToolCall::new(
            "tc1",
            "write_file",
            serde_json::json!({"file_path": "/tmp/bar.rs", "content": "hello"}),
        )];
        let results = vec![ToolResult::success("tc1", serde_json::json!("ok"))];
        tracker.record_from_tool_calls(&tool_calls, &results);
        assert_eq!(tracker.render(), "- /tmp/bar.rs (written)\n");
    }

    #[test]
    fn record_from_tool_calls_edit_file() {
        let mut tracker = FileTracker::default();
        let tool_calls = vec![ToolCall::new(
            "tc1",
            "edit_file",
            serde_json::json!({"file_path": "/tmp/baz.rs"}),
        )];
        let results = vec![ToolResult::success("tc1", serde_json::json!("ok"))];
        tracker.record_from_tool_calls(&tool_calls, &results);
        assert_eq!(tracker.render(), "- /tmp/baz.rs (edited)\n");
    }

    #[test]
    fn record_from_kimi_tool_calls_uses_path_argument() {
        let mut tracker = FileTracker::default();
        let tool_calls = vec![
            ToolCall::new("tc1", "Read", serde_json::json!({"path": "/tmp/a.rs"})),
            ToolCall::new(
                "tc2",
                "Write",
                serde_json::json!({"path": "/tmp/b.rs", "content": "x"}),
            ),
            ToolCall::new("tc3", "Edit", serde_json::json!({"path": "/tmp/c.rs"})),
        ];
        let results = ["tc1", "tc2", "tc3"]
            .into_iter()
            .map(|id| ToolResult::success(id, serde_json::json!("ok")))
            .collect::<Vec<_>>();

        tracker.record_from_tool_calls(&tool_calls, &results);

        assert_eq!(
            tracker.render(),
            "- /tmp/a.rs (read)\n- /tmp/b.rs (written)\n- /tmp/c.rs (edited)\n"
        );
    }

    #[test]
    fn record_from_tool_calls_skips_errors() {
        let mut tracker = FileTracker::default();
        let tool_calls = vec![ToolCall::new(
            "tc1",
            "read_file",
            serde_json::json!({"file_path": "/tmp/missing.rs"}),
        )];
        let results = vec![ToolResult::error("tc1", "File not found")];
        tracker.record_from_tool_calls(&tool_calls, &results);
        assert!(tracker.is_empty());
    }

    #[test]
    fn record_from_tool_calls_apply_patch_added() {
        let mut tracker = FileTracker::default();
        let tool_calls = vec![ToolCall::new(
            "tc1",
            "apply_patch",
            serde_json::json!({"patch": "..."}),
        )];
        let results = vec![ToolResult::success(
            "tc1",
            serde_json::json!(
                "Success. Updated the following files:\nA src/new.rs\nM src/old.rs\n"
            ),
        )];
        tracker.record_from_tool_calls(&tool_calls, &results);
        assert_eq!(
            tracker.render(),
            "- src/new.rs (written)\n- src/old.rs (edited)\n"
        );
    }

    #[test]
    fn is_empty_and_file_count() {
        let mut tracker = FileTracker::default();
        assert!(tracker.is_empty());
        assert_eq!(tracker.file_count(), 0);

        tracker.record_read("a.rs");
        tracker.record_write("b.rs");
        assert!(!tracker.is_empty());
        assert_eq!(tracker.file_count(), 2);
    }

    #[test]
    fn record_from_tool_calls_ignores_unknown_tools() {
        let mut tracker = FileTracker::default();
        let tool_calls = vec![ToolCall::new(
            "tc1",
            "shell",
            serde_json::json!({"command": "ls"}),
        )];
        let results = vec![ToolResult::success(
            "tc1",
            serde_json::json!("file1\nfile2"),
        )];
        tracker.record_from_tool_calls(&tool_calls, &results);
        assert!(tracker.is_empty());
    }
}
