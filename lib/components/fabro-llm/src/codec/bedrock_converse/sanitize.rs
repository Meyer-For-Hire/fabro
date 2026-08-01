//! Bedrock Converse tool identifier sanitization.
//!
//! Tool names must match `[a-zA-Z0-9_-]+`; tool-use IDs additionally allow
//! `.` and `:`. Both are limited to 64 characters. These helpers rewrite only
//! the Bedrock wire view: the canonical transcript retains provider output
//! verbatim. The encoder routes every tool block through its
//! `tool_use_block`/`tool_result_block` constructors so `toolUse` and
//! `toolResult` blocks remain paired.

use sha2::{Digest, Sha256};

const MAX_LENGTH: usize = 64;
const HASH_HEX_LENGTH: usize = 16;
const PREFIX_LENGTH: usize = MAX_LENGTH - 1 - HASH_HEX_LENGTH;

pub(super) fn tool_name(name: &str) -> String {
    sanitize(name, "unknown_tool", is_tool_name_char)
}

pub(super) fn tool_use_id(id: &str) -> String {
    sanitize(id, "unknown_tool_use_id", is_tool_use_id_char)
}

fn sanitize(value: &str, empty_fallback: &'static str, is_allowed: fn(char) -> bool) -> String {
    if value.is_empty() {
        return empty_fallback.to_string();
    }

    let sanitized: String = value
        .chars()
        .map(|character| {
            if is_allowed(character) {
                character
            } else {
                '_'
            }
        })
        .collect();

    if sanitized.len() <= MAX_LENGTH {
        sanitized
    } else {
        truncate_with_hash(&sanitized, value)
    }
}

fn is_tool_name_char(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '_' | '-')
}

fn is_tool_use_id_char(character: char) -> bool {
    is_tool_name_char(character) || matches!(character, '.' | ':')
}

fn truncate_with_hash(sanitized: &str, original: &str) -> String {
    debug_assert!(sanitized.is_ascii());
    let digest = Sha256::digest(original.as_bytes());
    let digest_hex = format!("{digest:x}");
    format!(
        "{}-{}",
        &sanitized[..PREFIX_LENGTH],
        &digest_hex[..HASH_HEX_LENGTH]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_values_pass_through_unchanged() {
        for name in ["search", "TaskList", "a-b_c9"] {
            assert_eq!(tool_name(name), name);
        }

        let max_length = "a".repeat(64);
        assert_eq!(tool_name(&max_length), max_length);

        let id = "functions.read_file:4";
        assert_eq!(tool_use_id(id), id);
        assert_eq!(tool_name(id), "functions_read_file_4");
    }

    #[test]
    fn invalid_characters_are_replaced() {
        assert_eq!(tool_name("search???"), "search___");
        assert_eq!(tool_name("bad name"), "bad_name");
        assert_eq!(tool_use_id("bad id!"), "bad_id_");
    }

    #[test]
    fn non_ascii_characters_become_single_underscores() {
        let sanitized = tool_name("before🙂after");
        assert_eq!(sanitized, "before_after");
        assert!(sanitized.is_ascii());
    }

    #[test]
    fn empty_values_use_nonempty_fallbacks() {
        assert_eq!(tool_name(""), "unknown_tool");
        assert_eq!(tool_use_id(""), "unknown_tool_use_id");
    }

    #[test]
    fn overlength_values_use_deterministic_hash_suffixes() {
        let boundary = "a".repeat(65);
        let first = tool_name(&boundary);
        let second = tool_name(&boundary);
        assert_eq!(first, second);
        assert_eq!(first.len(), 64);
        assert!(
            first
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        );

        let shared_prefix = "x".repeat(99);
        let left = tool_name(&format!("{shared_prefix}a"));
        let right = tool_name(&format!("{shared_prefix}b"));
        assert_ne!(left, right);
    }
}
