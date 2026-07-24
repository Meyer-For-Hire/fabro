//! Shared prompt-cache policy: whether a request opts into explicit
//! Anthropic-style caching and where the conversation breakpoint lands.
//! Dialect codecs apply these decisions to their own wire shapes.

/// Anthropic-style `cache_control` annotation.
#[derive(serde::Serialize, Clone)]
pub(crate) struct CacheControl {
    #[serde(rename = "type")]
    pub kind: String,
}

impl CacheControl {
    pub(crate) fn ephemeral() -> Self {
        Self {
            kind: "ephemeral".to_string(),
        }
    }
}

/// Whether automatic prompt caching applies to this request: the
/// `provider_options.<namespace>.auto_cache` opt-out defaults to enabled.
pub(crate) fn auto_cache_enabled(
    provider_options: Option<&serde_json::Value>,
    namespace: &str,
) -> bool {
    provider_options
        .and_then(|opts| opts.get(namespace))
        .and_then(|ns| ns.get("auto_cache"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true)
}

/// Index of the message carrying the conversation-prefix breakpoint: the
/// second-to-last user turn, so each iteration of an agent loop reuses the
/// prefix cached by the previous one. `user_turns[i]` is true when message
/// `i` advances the user side of the conversation (plain user messages, plus
/// tool results on dialects where they are separate messages). `None` until
/// the conversation has at least two user turns.
pub(crate) fn conversation_breakpoint_index(user_turns: &[bool]) -> Option<usize> {
    let indices: Vec<usize> = user_turns
        .iter()
        .enumerate()
        .filter_map(|(i, &is_user)| is_user.then_some(i))
        .collect();
    indices.len().checked_sub(2).map(|nth| indices[nth])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_cache_enabled_by_default() {
        assert!(auto_cache_enabled(None, "anthropic"));
    }

    #[test]
    fn auto_cache_enabled_when_true() {
        let opts = serde_json::json!({"anthropic": {"auto_cache": true}});
        assert!(auto_cache_enabled(Some(&opts), "anthropic"));
    }

    #[test]
    fn auto_cache_disabled_when_false() {
        let opts = serde_json::json!({"openrouter": {"auto_cache": false}});
        assert!(!auto_cache_enabled(Some(&opts), "openrouter"));
    }

    #[test]
    fn auto_cache_enabled_when_key_missing() {
        let opts = serde_json::json!({"anthropic": {}});
        assert!(auto_cache_enabled(Some(&opts), "anthropic"));
    }

    #[test]
    fn auto_cache_reads_only_its_own_namespace() {
        let opts = serde_json::json!({"openrouter": {"auto_cache": false}});
        assert!(auto_cache_enabled(Some(&opts), "anthropic"));
    }

    #[test]
    fn conversation_breakpoint_none_below_two_user_turns() {
        assert_eq!(conversation_breakpoint_index(&[]), None);
        assert_eq!(conversation_breakpoint_index(&[true]), None);
        assert_eq!(conversation_breakpoint_index(&[true, false, false]), None);
    }

    #[test]
    fn conversation_breakpoint_with_exactly_two_user_turns() {
        assert_eq!(conversation_breakpoint_index(&[true, false, true]), Some(0));
    }

    #[test]
    fn conversation_breakpoint_targets_second_to_last_user_turn() {
        let turns = [true, false, true, false, true];
        assert_eq!(conversation_breakpoint_index(&turns), Some(2));
    }
}
