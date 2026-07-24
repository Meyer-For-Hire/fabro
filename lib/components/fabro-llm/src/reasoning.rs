//! Normalization of provider reasoning material into [`ReasoningOutput`].
//!
//! Every provider that returns readable reasoning does it differently, and
//! several return more than one channel at once. This module reduces the
//! final response's content parts to the two normalized fields without
//! reaching into opaque material (signatures, item IDs, encrypted payloads)
//! and without failing a completion it cannot classify.
//!
//! Parsing is deliberately tolerant: provider payloads are read as
//! `serde_json::Value` with optional lookups, so unknown detail variants,
//! missing members, extra members, and unexpected member types are ignored
//! rather than surfaced as errors.

use fabro_types::{ContentPart, ReasoningOutput};

/// Separator between distinct complete reasoning blocks. Fragments of one
/// logical block are coalesced by the streaming decoders before they reach
/// this module.
const BLOCK_SEPARATOR: &str = "\n\n";

/// Detail types whose payload is opaque and must never reach the public
/// normalized fields.
fn is_opaque_detail_type(detail_type: &str) -> bool {
    detail_type.contains("encrypted") || detail_type.contains("redacted")
}

/// Readable blocks collected per normalized field.
///
/// Explicit blocks come from a channel with documented reasoning semantics.
/// Fallback blocks come from flattened provider strings, which aggregators
/// commonly duplicate alongside a structured channel; they only fill a
/// summary that no explicit block produced.
#[derive(Default)]
struct Blocks {
    explicit_summary: Vec<String>,
    explicit_trace:   Vec<String>,
    fallback_summary: Vec<String>,
}

impl Blocks {
    fn into_output(self) -> Option<ReasoningOutput> {
        let summary =
            join_blocks(&self.explicit_summary).or_else(|| join_blocks(&self.fallback_summary));
        let trace = join_blocks(&self.explicit_trace);
        let output = ReasoningOutput { summary, trace };
        (!output.is_empty()).then_some(output)
    }
}

/// Join complete blocks in provider order, dropping empty and
/// whitespace-only fragments. Retained text is never trimmed or rewritten.
fn join_blocks(blocks: &[String]) -> Option<String> {
    let joined = blocks
        .iter()
        .filter(|block| !block.trim().is_empty())
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(BLOCK_SEPARATOR);
    (!joined.is_empty()).then_some(joined)
}

/// Read a text-bearing member, preferring the member the provider's
/// documented semantics name and accepting the other readable spelling.
fn readable_member(entry: &serde_json::Value, preferred: &str) -> Option<String> {
    let other = if preferred == "text" {
        "summary"
    } else {
        "text"
    };
    entry
        .get(preferred)
        .and_then(serde_json::Value::as_str)
        .or_else(|| entry.get(other).and_then(serde_json::Value::as_str))
        .map(str::to_string)
}

/// Extract readable text from an OpenAI Responses `reasoning` output item.
///
/// `summary[].text` is the model-authored summary; `content[]` entries typed
/// `reasoning_text` are the verbatim trace. `encrypted_content`, `id`, and
/// `status` are opaque and ignored.
fn collect_openai_reasoning_item(item: &serde_json::Value, blocks: &mut Blocks) {
    if let Some(entries) = item.get("summary").and_then(serde_json::Value::as_array) {
        for entry in entries {
            if let Some(text) = entry.as_str() {
                blocks.explicit_summary.push(text.to_string());
            } else if let Some(text) = entry.get("text").and_then(serde_json::Value::as_str) {
                blocks.explicit_summary.push(text.to_string());
            }
        }
    }
    if let Some(entries) = item.get("content").and_then(serde_json::Value::as_array) {
        for entry in entries {
            let Some(text) = entry.get("text").and_then(serde_json::Value::as_str) else {
                continue;
            };
            let entry_type = entry
                .get("type")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            if entry_type == "reasoning_text" {
                blocks.explicit_trace.push(text.to_string());
            } else if !is_opaque_detail_type(entry_type) {
                // Readable text from the reasoning channel that carries no
                // recognized classification is treated as a summary.
                blocks.explicit_summary.push(text.to_string());
            }
        }
    }
}

/// Extract readable text from OpenAI-compatible `reasoning_details` entries.
fn collect_reasoning_details(details: &serde_json::Value, blocks: &mut Blocks) {
    let Some(entries) = details.as_array() else {
        return;
    };
    for entry in entries {
        let detail_type = entry
            .get("type")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        if is_opaque_detail_type(detail_type) {
            continue;
        }
        match detail_type {
            "reasoning.text" => {
                if let Some(text) = readable_member(entry, "text") {
                    blocks.explicit_trace.push(text);
                }
            }
            // Documented summaries and unknown detail variants both come
            // from an established reasoning channel, so a readable member
            // is kept as a summary either way.
            _ => {
                if let Some(text) = readable_member(entry, "summary") {
                    blocks.explicit_summary.push(text);
                }
            }
        }
    }
}

/// Normalize the content parts of a final response into readable reasoning.
///
/// Returns `None` when the response carries no readable reasoning, so an
/// event without reasoning keeps its previous serialized shape.
pub(crate) fn normalize(content: &[ContentPart]) -> Option<ReasoningOutput> {
    let mut blocks = Blocks::default();
    for part in content {
        match part {
            ContentPart::Thinking(thinking) if !thinking.redacted => {
                blocks.fallback_summary.push(thinking.text.clone());
            }
            ContentPart::Other { kind, data } if kind == ContentPart::OPENAI_REASONING => {
                collect_openai_reasoning_item(data, &mut blocks);
            }
            ContentPart::Other { kind, data }
                if kind == ContentPart::OPENAI_COMPAT_REASONING_DETAILS =>
            {
                collect_reasoning_details(data, &mut blocks);
            }
            _ => {}
        }
    }
    blocks.into_output()
}

#[cfg(test)]
mod tests {
    use fabro_types::ThinkingData;
    use serde_json::json;

    use super::*;

    fn thinking(text: &str) -> ContentPart {
        ContentPart::Thinking(ThinkingData {
            text:      text.to_string(),
            signature: None,
            redacted:  false,
        })
    }

    fn openai_reasoning(item: serde_json::Value) -> ContentPart {
        ContentPart::Other {
            kind: ContentPart::OPENAI_REASONING.to_string(),
            data: item,
        }
    }

    fn reasoning_details(details: serde_json::Value) -> ContentPart {
        ContentPart::Other {
            kind: ContentPart::OPENAI_COMPAT_REASONING_DETAILS.to_string(),
            data: details,
        }
    }

    #[test]
    fn non_redacted_thinking_becomes_a_summary() {
        let output = normalize(&[thinking("weighing the options")]).unwrap();
        assert_eq!(output.summary.as_deref(), Some("weighing the options"));
        assert!(output.trace.is_none());
    }

    #[test]
    fn redacted_thinking_yields_no_readable_reasoning() {
        let redacted = ContentPart::Thinking(ThinkingData {
            text:      "AAAAopaque".to_string(),
            signature: Some("sig".to_string()),
            redacted:  true,
        });
        assert!(normalize(&[redacted]).is_none());
    }

    #[test]
    fn responses_item_with_summary_and_reasoning_text_produces_both_fields() {
        let output = normalize(&[openai_reasoning(json!({
            "type": "reasoning",
            "id": "rs_1",
            "encrypted_content": "gAAAAA",
            "summary": [{"type": "summary_text", "text": "inspect first"}],
            "content": [{"type": "reasoning_text", "text": "step one"}],
        }))])
        .unwrap();
        assert_eq!(output.summary.as_deref(), Some("inspect first"));
        assert_eq!(output.trace.as_deref(), Some("step one"));
    }

    #[test]
    fn responses_blocks_join_in_provider_order() {
        let output = normalize(&[openai_reasoning(json!({
            "summary": [
                {"type": "summary_text", "text": "first"},
                {"type": "summary_text", "text": "second"},
            ],
        }))])
        .unwrap();
        assert_eq!(output.summary.as_deref(), Some("first\n\nsecond"));
    }

    #[test]
    fn structured_details_produce_summary_and_trace() {
        let output = normalize(&[reasoning_details(json!([
            {"type": "reasoning.summary", "summary": "checked the parser"},
            {"type": "reasoning.text", "text": "read convert.rs", "signature": "sig"},
        ]))])
        .unwrap();
        assert_eq!(output.summary.as_deref(), Some("checked the parser"));
        assert_eq!(output.trace.as_deref(), Some("read convert.rs"));
    }

    #[test]
    fn encrypted_details_are_excluded() {
        let output = normalize(&[reasoning_details(json!([
            {"type": "reasoning.encrypted", "data": "gAAAAAsecret", "format": "openai-responses-v1"},
            {"type": "reasoning.summary", "summary": "visible"},
        ])),])
        .unwrap();
        assert_eq!(output.summary.as_deref(), Some("visible"));
        assert!(output.trace.is_none());
    }

    #[test]
    fn encrypted_only_details_produce_no_reasoning() {
        assert!(
            normalize(&[reasoning_details(json!([
                {"type": "reasoning.encrypted", "data": "gAAAAAsecret"},
            ]))])
            .is_none()
        );
    }

    #[test]
    fn unknown_detail_variants_are_treated_as_summary() {
        let output = normalize(&[reasoning_details(json!([
            {"type": "reasoning.future", "text": "new channel"},
        ]))])
        .unwrap();
        assert_eq!(output.summary.as_deref(), Some("new channel"));
    }

    #[test]
    fn malformed_details_are_ignored_without_failing() {
        assert!(normalize(&[reasoning_details(json!("not-an-array"))]).is_none());
        assert!(
            normalize(&[reasoning_details(json!([
                42,
                {"type": "reasoning.summary", "summary": 7},
                {"no_type": true},
            ]))])
            .is_none()
        );
    }

    #[test]
    fn structured_details_suppress_a_duplicate_flattened_value() {
        let output = normalize(&[
            reasoning_details(json!([
                {"type": "reasoning.summary", "summary": "checked the parser"},
            ])),
            thinking("checked the parser"),
        ])
        .unwrap();
        assert_eq!(output.summary.as_deref(), Some("checked the parser"));
        assert!(output.trace.is_none());
    }

    #[test]
    fn trace_only_detail_lets_the_fallback_fill_the_summary() {
        let output = normalize(&[
            reasoning_details(json!([{"type": "reasoning.text", "text": "verbatim"}])),
            thinking("flattened"),
        ])
        .unwrap();
        assert_eq!(output.summary.as_deref(), Some("flattened"));
        assert_eq!(output.trace.as_deref(), Some("verbatim"));
    }

    #[test]
    fn whitespace_only_fragments_do_not_create_reasoning() {
        assert!(normalize(&[thinking("   \n ")]).is_none());
    }

    #[test]
    fn non_empty_text_is_preserved_verbatim() {
        let output = normalize(&[thinking("  indented thought\n")]).unwrap();
        assert_eq!(output.summary.as_deref(), Some("  indented thought\n"));
    }

    #[test]
    fn unrelated_content_parts_are_ignored() {
        let parts = vec![ContentPart::text("answer"), ContentPart::Other {
            kind: ContentPart::OPENAI_MESSAGE.to_string(),
            data: json!({"type": "message", "content": [{"text": "answer"}]}),
        }];
        assert!(normalize(&parts).is_none());
    }
}
