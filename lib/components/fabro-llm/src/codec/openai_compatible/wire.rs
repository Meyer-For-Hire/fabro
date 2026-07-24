//! Serde types mirroring the OpenAI Chat Completions wire shapes.

use crate::codec::cache::CacheControl;
use crate::codec::split_inclusive_token_total;
use crate::types::{ReasoningEffort, TokenCounts};

#[derive(serde::Serialize)]
pub(super) struct ApiRequest {
    pub model:            String,
    pub messages:         Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature:      Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens:       Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p:            Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<ReasoningEffort>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop:             Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools:            Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice:      Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format:  Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream:           Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_options:   Option<StreamOptions>,
}

/// Streaming options. Chat Completions only emits the trailing usage chunk
/// when the request opts in, so without this a streamed response reports zero
/// tokens and costs are estimated at $0.
#[derive(serde::Serialize)]
pub(super) struct StreamOptions {
    pub include_usage: bool,
}

#[derive(serde::Serialize)]
pub(super) struct ChatMessage {
    pub role:              String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content:           Option<ChatContent>,
    /// Reasoning/thinking content echoed back for providers that require it
    /// (Kimi).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id:      Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls:        Option<Vec<ChatToolCall>>,
}

/// Message content: plain text, or text parts when a part carries a
/// `cache_control` breakpoint (aggregators fronting Anthropic models forward
/// it upstream). Unmarked messages keep the plain-string form for maximum
/// compatibility with strict Chat Completions servers.
#[derive(serde::Serialize)]
#[serde(untagged)]
pub(super) enum ChatContent {
    Text(String),
    Parts(Vec<ChatTextPart>),
}

#[derive(serde::Serialize)]
pub(super) struct ChatTextPart {
    #[serde(rename = "type")]
    pub kind:          String,
    pub text:          String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
}

impl ChatContent {
    /// Plain-text view for assertions.
    #[cfg(test)]
    pub(super) fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text.as_str()),
            Self::Parts(_) => None,
        }
    }

    /// Mark this content as a prompt-cache breakpoint, converting to parts
    /// form so the annotation has somewhere to live.
    pub(super) fn mark_cache_breakpoint(&mut self) {
        match self {
            Self::Text(text) => {
                *self = Self::Parts(vec![ChatTextPart {
                    kind:          "text".to_string(),
                    text:          std::mem::take(text),
                    cache_control: Some(CacheControl::ephemeral()),
                }]);
            }
            Self::Parts(parts) => {
                if let Some(last) = parts.last_mut() {
                    last.cache_control = Some(CacheControl::ephemeral());
                }
            }
        }
    }
}

#[derive(serde::Serialize)]
pub(super) struct ChatToolCall {
    pub id:       String,
    #[serde(rename = "type")]
    pub kind:     String,
    pub function: ChatFunction,
}

#[derive(serde::Serialize)]
pub(super) struct ChatFunction {
    pub name:      String,
    pub arguments: String,
}

// --- Response types (non-streaming) ---

#[derive(serde::Deserialize)]
pub(super) struct ApiResponse {
    pub id:      String,
    pub model:   String,
    pub choices: Vec<ApiChoice>,
    pub usage:   Option<ApiUsage>,
}

#[derive(serde::Deserialize)]
pub(super) struct ApiChoice {
    pub message:       ApiChoiceMessage,
    pub finish_reason: Option<String>,
}

#[derive(serde::Deserialize)]
pub(super) struct ApiChoiceMessage {
    pub content:           Option<String>,
    pub reasoning_content: Option<String>,
    /// OpenRouter's normalized spelling for reasoning text.
    pub reasoning:         Option<String>,
    pub tool_calls:        Option<Vec<ApiToolCall>>,
}

impl ApiChoiceMessage {
    pub(super) fn reasoning(&self) -> Option<&str> {
        self.reasoning_content
            .as_deref()
            .or(self.reasoning.as_deref())
    }
}

#[derive(serde::Deserialize)]
pub(super) struct ApiToolCall {
    pub id:       String,
    pub function: ApiFunction,
}

#[derive(serde::Deserialize)]
pub(super) struct ApiFunction {
    pub name:      String,
    pub arguments: String,
}

#[derive(serde::Deserialize)]
#[allow(
    clippy::struct_field_names,
    reason = "Field names mirror the provider API payload."
)]
pub(super) struct ApiUsage {
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    /// Tolerant superset: aggregator dialects (OpenRouter) report in-band
    /// USD cost and cache/reasoning token detail. Absent on plain providers.
    #[serde(default)]
    pub cost: Option<f64>,
    #[serde(default)]
    pub prompt_tokens_details: Option<PromptTokensDetails>,
    #[serde(default)]
    pub completion_tokens_details: Option<CompletionTokensDetails>,
}

#[derive(serde::Deserialize)]
pub(super) struct PromptTokensDetails {
    #[serde(default)]
    pub cached_tokens:      Option<i64>,
    /// OpenRouter-specific: explicit-cache write tokens.
    #[serde(default)]
    pub cache_write_tokens: Option<i64>,
}

#[derive(serde::Deserialize)]
pub(super) struct CompletionTokensDetails {
    #[serde(default)]
    pub reasoning_tokens: Option<i64>,
}

impl ApiUsage {
    /// Normalize into disjoint [`TokenCounts`] buckets: cached and
    /// cache-write detail tokens are subtracted out of `input_tokens`, and
    /// reasoning tokens out of `output_tokens`, mirroring the
    /// `openai_responses` convention.
    pub(super) fn token_counts(&self) -> TokenCounts {
        let cached_detail = self
            .prompt_tokens_details
            .as_ref()
            .and_then(|d| d.cached_tokens)
            .unwrap_or(0);
        let cache_write_detail = self
            .prompt_tokens_details
            .as_ref()
            .and_then(|d| d.cache_write_tokens)
            .unwrap_or(0);
        let reasoning_detail = self
            .completion_tokens_details
            .as_ref()
            .and_then(|d| d.reasoning_tokens)
            .unwrap_or(0);
        let (uncached_input, cached) =
            split_inclusive_token_total(self.prompt_tokens, cached_detail);
        let (input_tokens, cache_write) =
            split_inclusive_token_total(uncached_input, cache_write_detail);
        let (output_tokens, reasoning) =
            split_inclusive_token_total(self.completion_tokens, reasoning_detail);
        TokenCounts {
            input_tokens,
            output_tokens,
            reasoning_tokens: reasoning,
            cache_read_tokens: cached,
            cache_write_tokens: cache_write,
        }
    }
}

// --- Streaming response types ---

#[derive(serde::Deserialize)]
pub(super) struct StreamChunk {
    pub id:      Option<String>,
    pub model:   Option<String>,
    pub choices: Option<Vec<StreamChoice>>,
    pub usage:   Option<ApiUsage>,
}

#[derive(serde::Deserialize)]
pub(super) struct StreamChoice {
    pub delta:         Option<StreamDelta>,
    pub finish_reason: Option<String>,
}

#[derive(serde::Deserialize)]
pub(super) struct StreamDelta {
    pub content:           Option<String>,
    /// Reasoning/thinking content (used by Kimi and other reasoning models).
    pub reasoning_content: Option<String>,
    /// OpenRouter's normalized spelling for reasoning text.
    pub reasoning:         Option<String>,
    pub tool_calls:        Option<Vec<StreamToolCall>>,
}

impl StreamDelta {
    pub(super) fn reasoning(&self) -> Option<&str> {
        self.reasoning_content
            .as_deref()
            .or(self.reasoning.as_deref())
    }
}

#[derive(serde::Deserialize)]
pub(super) struct StreamToolCall {
    pub index:    usize,
    pub id:       Option<String>,
    pub function: Option<StreamFunction>,
}

#[derive(serde::Deserialize)]
pub(super) struct StreamFunction {
    pub name:      Option<String>,
    pub arguments: Option<String>,
}

// --- Accumulated tool call state for streaming ---

pub(super) struct AccumulatedToolCall {
    pub id:        String,
    pub name:      String,
    pub arguments: String,
    pub started:   bool,
}

#[cfg(test)]
mod tests {
    use super::{ApiResponse, ApiUsage, ChatContent, ChatTextPart, StreamChunk};
    use crate::codec::cache::CacheControl;
    use crate::types::TokenCounts;

    #[test]
    fn chat_content_text_serializes_as_plain_string() {
        let content = ChatContent::Text("Hello".to_string());
        assert_eq!(
            serde_json::to_value(&content).unwrap(),
            serde_json::json!("Hello")
        );
    }

    #[test]
    fn mark_cache_breakpoint_converts_text_to_annotated_parts() {
        let mut content = ChatContent::Text("Hello".to_string());
        content.mark_cache_breakpoint();
        assert_eq!(
            serde_json::to_value(&content).unwrap(),
            serde_json::json!([{
                "type": "text",
                "text": "Hello",
                "cache_control": {"type": "ephemeral"}
            }])
        );
    }

    #[test]
    fn mark_cache_breakpoint_annotates_last_existing_part() {
        let mut content = ChatContent::Parts(vec![
            ChatTextPart {
                kind:          "text".to_string(),
                text:          "first".to_string(),
                cache_control: None,
            },
            ChatTextPart {
                kind:          "text".to_string(),
                text:          "second".to_string(),
                cache_control: Some(CacheControl::ephemeral()),
            },
        ]);
        content.mark_cache_breakpoint();
        let json = serde_json::to_value(&content).unwrap();
        assert!(json[0].get("cache_control").is_none());
        assert_eq!(json[1]["cache_control"]["type"], "ephemeral");
    }

    #[test]
    fn token_counts_bound_detail_to_parent_totals() {
        let usage: ApiUsage = serde_json::from_value(serde_json::json!({
            "prompt_tokens": 53,
            "completion_tokens": 59,
            "completion_tokens_details": {"reasoning_tokens": 66}
        }))
        .unwrap();

        assert_eq!(usage.token_counts(), TokenCounts {
            input_tokens: 53,
            output_tokens: 0,
            reasoning_tokens: 59,
            ..TokenCounts::default()
        });
    }

    #[test]
    fn reasoning_accepts_provider_and_openrouter_spellings() {
        let provider_response: ApiResponse = serde_json::from_value(serde_json::json!({
            "id": "response-1",
            "model": "reasoning-model",
            "choices": [{
                "message": {
                    "content": null,
                    "reasoning_content": "provider reasoning"
                },
                "finish_reason": "stop"
            }]
        }))
        .unwrap();
        assert_eq!(
            provider_response.choices[0].message.reasoning(),
            Some("provider reasoning")
        );

        let openrouter_response: ApiResponse = serde_json::from_value(serde_json::json!({
            "id": "response-2",
            "model": "reasoning-model",
            "choices": [{
                "message": {
                    "content": null,
                    "reasoning": "OpenRouter reasoning"
                },
                "finish_reason": "stop"
            }]
        }))
        .unwrap();
        assert_eq!(
            openrouter_response.choices[0].message.reasoning(),
            Some("OpenRouter reasoning")
        );
        let openrouter_chunk: StreamChunk = serde_json::from_value(serde_json::json!({
            "id": "response-2",
            "model": "reasoning-model",
            "choices": [{
                "delta": {"reasoning": "OpenRouter reasoning"},
                "finish_reason": null
            }]
        }))
        .unwrap();
        assert_eq!(
            openrouter_chunk.choices.unwrap()[0]
                .delta
                .as_ref()
                .unwrap()
                .reasoning(),
            Some("OpenRouter reasoning")
        );
    }
}
