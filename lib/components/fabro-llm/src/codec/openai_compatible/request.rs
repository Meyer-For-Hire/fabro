//! Request encoding: canonical `Request` → Chat Completions body.

use super::translate;
use super::wire::{ApiRequest, ChatMessage, StreamOptions};
use crate::codec::{CodecCtx, EncodedRequest, cache, merge_named_provider_options};
use crate::error::Error;

/// Known `provider_options.<provider_name>` keys the codec consumes itself;
/// not re-merged into the body.
const KNOWN_OPTION_KEYS: &[&str] = &["auto_cache"];

/// Build the Chat Completions request for `ctx.request`. `stream` toggles the
/// `stream` body field and the `stream_options.include_usage` opt-in that makes
/// providers emit the trailing usage chunk. The body is assembled as a
/// `serde_json::Value` so `provider_options.<provider_name>` fields can be
/// merged in before sending.
///
/// Returns an error when the request contains a custom tool definition, which
/// the Chat Completions tool envelope cannot represent.
pub(super) fn encode(ctx: &CodecCtx<'_>, stream: bool) -> Result<EncodedRequest, Error> {
    let request = ctx.request;
    let mut chat_messages = translate::translate_messages(&request.messages);
    if explicit_cache_breakpoints(ctx) {
        apply_cache_breakpoints(&mut chat_messages);
    }
    let tools = request
        .tools
        .as_ref()
        .map(|t| translate::translate_tools(t))
        .transpose()?;
    let tool_choice = request
        .tool_choice
        .as_ref()
        .map(translate::translate_tool_choice);
    let response_format = request
        .response_format
        .as_ref()
        .map(translate::translate_response_format);
    let (temperature, top_p) = if ctx
        .model
        .is_none_or(fabro_model::Model::supports_sampling_params)
    {
        (request.temperature, request.top_p)
    } else {
        (None, None)
    };

    let api_request = ApiRequest {
        model: ctx.deployment_id.to_string(),
        messages: chat_messages,
        temperature,
        max_tokens: request.max_tokens,
        top_p,
        reasoning_effort: request.reasoning_effort,
        stop: request.stop_sequences.clone(),
        tools,
        tool_choice,
        response_format,
        stream: stream.then_some(true),
        stream_options: stream.then_some(StreamOptions {
            include_usage: true,
        }),
    };

    let mut body = serde_json::to_value(&api_request).unwrap_or_default();
    merge_provider_options(
        &mut body,
        request.provider_options.as_ref(),
        ctx.provider_name,
    );

    Ok(EncodedRequest {
        body,
        endpoint: "/chat/completions".to_string(),
        headers: Vec::new(),
    })
}

/// Whether this request opts into Anthropic-style explicit cache breakpoints:
/// the catalog row declares the mechanism and the request hasn't disabled
/// `auto_cache` under this provider's options namespace.
fn explicit_cache_breakpoints(ctx: &CodecCtx<'_>) -> bool {
    ctx.model
        .is_some_and(|m| m.features.prompt_cache && m.features.cache_control_breakpoints)
        && cache::auto_cache_enabled(ctx.request.provider_options.as_ref(), ctx.provider_name)
}

/// Mark the cacheable prefix: the last system message (upstream, tools and
/// system precede the conversation, so this breakpoint covers them too) and
/// the second-to-last user turn. Tool results count as user turns — they ride
/// in user messages on the upstream Anthropic wire.
fn apply_cache_breakpoints(messages: &mut [ChatMessage]) {
    if let Some(system) = messages.iter_mut().rev().find(|m| m.role == "system") {
        if let Some(content) = system.content.as_mut() {
            content.mark_cache_breakpoint();
        }
    }

    let user_turns: Vec<bool> = messages
        .iter()
        .map(|m| m.role == "user" || m.role == "tool")
        .collect();
    if let Some(idx) = cache::conversation_breakpoint_index(&user_turns) {
        if let Some(content) = messages[idx].content.as_mut() {
            content.mark_cache_breakpoint();
        }
    }
}

/// Merge `provider_options.<provider_name>` fields into the serialized API
/// request body.
///
/// The provider name is configurable (e.g. "groq", "together", "moonshot"),
/// allowing each instance to have its own namespace in `provider_options`.
pub(super) fn merge_provider_options(
    body: &mut serde_json::Value,
    provider_options: Option<&serde_json::Value>,
    provider_name: &str,
) {
    merge_named_provider_options(body, provider_options, provider_name, KNOWN_OPTION_KEYS);
}

#[cfg(test)]
mod tests {
    use fabro_model::{Catalog, ProviderId};

    use super::super::wire::ApiRequest;
    use super::*;
    use crate::codec::CodecParams;
    use crate::types::{Message, ReasoningEffort, Request, ToolDefinition};

    fn minimal_request() -> Request {
        Request {
            model:            "llama-3.1-70b".to_string(),
            messages:         vec![Message::user("Hello")],
            provider:         None,
            tools:            None,
            tool_choice:      None,
            response_format:  None,
            temperature:      None,
            top_p:            None,
            max_tokens:       None,
            stop_sequences:   None,
            reasoning_effort: None,
            speed:            None,
            metadata:         None,
            provider_options: None,
        }
    }

    /// Encode `request` through the codec with `deployment_id == request.model`
    /// (the no-catalog case) and return the body.
    fn encode_body(request: &Request, provider_name: &str, stream: bool) -> serde_json::Value {
        let params = CodecParams::default();
        let deployment_id = request.model.clone();
        let ctx = CodecCtx {
            request,
            provider_name,
            deployment_id: &deployment_id,
            model: None,
            params: &params,
        };
        encode(&ctx, stream).unwrap().body
    }

    #[test]
    fn api_request_stream_field_serialization() {
        let req = ApiRequest {
            model:            "test".into(),
            messages:         vec![],
            temperature:      None,
            max_tokens:       None,
            top_p:            None,
            reasoning_effort: None,
            stop:             None,
            tools:            None,
            tool_choice:      None,
            response_format:  None,
            stream:           Some(true),
            stream_options:   None,
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["stream"], true);

        let req_no_stream = ApiRequest {
            model:            "test".into(),
            messages:         vec![],
            temperature:      None,
            max_tokens:       None,
            top_p:            None,
            reasoning_effort: None,
            stop:             None,
            tools:            None,
            tool_choice:      None,
            response_format:  None,
            stream:           None,
            stream_options:   None,
        };
        let json_no_stream = serde_json::to_value(&req_no_stream).unwrap();
        assert!(json_no_stream.get("stream").is_none());
    }

    #[test]
    fn encode_uses_deployment_id_as_model() {
        let request = minimal_request();
        let params = CodecParams::default();
        let deployment_id = "acme/model-large".to_string();
        let ctx = CodecCtx {
            request:       &request,
            provider_name: "acme",
            deployment_id: &deployment_id,
            model:         None,
            params:        &params,
        };
        let body = encode(&ctx, false).unwrap().body;
        assert_eq!(body["model"], "acme/model-large");
    }

    #[test]
    fn encode_serializes_reasoning_effort_at_top_level() {
        let mut request = minimal_request();
        request.reasoning_effort = Some(ReasoningEffort::High);

        let body = encode_body(&request, "moonshot", false);

        assert_eq!(body["reasoning_effort"], "high");
    }

    #[test]
    fn encode_omits_sampling_params_for_models_that_reject_them() {
        let model = Catalog::builtin()
            .get_on_provider(&ProviderId::new("moonshot"), "kimi-k3")
            .unwrap();
        let mut request = minimal_request();
        request.model = model.id.to_string();
        request.temperature = Some(0.7);
        request.top_p = Some(0.9);
        let params = CodecParams::default();
        let ctx = CodecCtx {
            request:       &request,
            provider_name: "moonshot",
            deployment_id: model.id.as_str(),
            model:         Some(model),
            params:        &params,
        };

        let body = encode(&ctx, false).unwrap().body;

        assert!(body.get("temperature").is_none());
        assert!(body.get("top_p").is_none());
    }

    #[test]
    fn encode_rejects_custom_tool_definitions() {
        let mut request = minimal_request();
        request.tools = Some(vec![ToolDefinition::custom(
            "apply_patch",
            "Apply a patch",
            serde_json::json!({"type": "grammar"}),
        )]);
        let params = CodecParams::default();
        let deployment_id = request.model.clone();
        let ctx = CodecCtx {
            request:       &request,
            provider_name: "moonshot",
            deployment_id: &deployment_id,
            model:         None,
            params:        &params,
        };

        let Err(error) = encode(&ctx, false) else {
            panic!("custom tool definition should be rejected");
        };
        assert!(matches!(
            error,
            Error::Configuration { message, source: None }
                if message.contains("custom tool definition 'apply_patch'")
        ));
    }

    #[test]
    fn provider_options_none_produces_standard_body() {
        let request = minimal_request();
        let body = encode_body(&request, "groq", false);
        assert_eq!(body["model"], "llama-3.1-70b");
        assert!(body.get("stream").is_none());
    }

    #[test]
    fn provider_options_matching_name_merged() {
        let mut request = minimal_request();
        request.provider_options = Some(serde_json::json!({
            "groq": {
                "frequency_penalty": 0.5,
                "presence_penalty": 0.3
            }
        }));
        let body = encode_body(&request, "groq", false);
        assert_eq!(body["frequency_penalty"], 0.5);
        assert_eq!(body["presence_penalty"], 0.3);
    }

    #[test]
    fn provider_options_different_name_ignored() {
        let mut request = minimal_request();
        request.provider_options = Some(serde_json::json!({
            "together": {
                "repetition_penalty": 1.2
            }
        }));
        let body = encode_body(&request, "groq", false);
        assert!(body.get("repetition_penalty").is_none());
    }

    #[test]
    fn provider_options_uses_adapter_name() {
        let mut request = minimal_request();
        request.provider_options = Some(serde_json::json!({
            "together": {
                "repetition_penalty": 1.2
            }
        }));
        let body = encode_body(&request, "together", false);
        assert_eq!(body["repetition_penalty"], 1.2);
    }

    #[test]
    fn provider_options_preserves_standard_fields() {
        let mut request = minimal_request();
        request.temperature = Some(0.7);
        request.max_tokens = Some(200);
        request.provider_options = Some(serde_json::json!({
            "groq": {
                "frequency_penalty": 0.5
            }
        }));
        let body = encode_body(&request, "groq", true);
        assert_eq!(body["temperature"], 0.7);
        assert_eq!(body["max_tokens"], 200);
        assert_eq!(body["stream"], true);
        assert_eq!(body["frequency_penalty"], 0.5);
    }

    #[test]
    fn provider_options_can_override_model() {
        let mut request = minimal_request();
        request.provider_options = Some(serde_json::json!({
            "groq": {
                "model": "custom-model"
            }
        }));
        let body = encode_body(&request, "groq", false);
        assert_eq!(body["model"], "custom-model");
    }

    #[test]
    fn merge_provider_options_with_non_object_value() {
        let mut body = serde_json::json!({"model": "test"});
        let opts = serde_json::json!({"groq": "not-an-object"});
        merge_provider_options(&mut body, Some(&opts), "groq");
        assert_eq!(body["model"], "test");
    }

    #[test]
    fn merge_provider_options_consumes_auto_cache_control_key() {
        let mut body = serde_json::json!({"model": "test"});
        let opts = serde_json::json!({"groq": {"auto_cache": false, "top_k": 5}});
        merge_provider_options(&mut body, Some(&opts), "groq");
        assert!(body.get("auto_cache").is_none());
        assert_eq!(body["top_k"], 5);
    }

    // --- apply_cache_breakpoints ---------------------------------------------

    fn chat_message(role: &str, text: &str) -> ChatMessage {
        ChatMessage {
            role:              role.to_string(),
            content:           Some(super::super::wire::ChatContent::Text(text.to_string())),
            reasoning_content: None,
            tool_call_id:      None,
            tool_calls:        None,
        }
    }

    fn marked(message: &ChatMessage) -> bool {
        let json = serde_json::to_value(message).unwrap();
        json["content"].is_array() && json["content"][0]["cache_control"]["type"] == "ephemeral"
    }

    #[test]
    fn cache_breakpoints_on_first_turn_mark_only_the_system_prompt() {
        let mut messages = vec![chat_message("system", "sys"), chat_message("user", "task")];
        apply_cache_breakpoints(&mut messages);
        assert!(marked(&messages[0]));
        assert!(!marked(&messages[1]));
    }

    #[test]
    fn cache_breakpoints_count_tool_results_as_user_turns() {
        let mut messages = vec![
            chat_message("system", "sys"),
            chat_message("user", "task"),
            chat_message("assistant", "calling a tool"),
            chat_message("tool", "tool output"),
            chat_message("assistant", "one more"),
            chat_message("tool", "more output"),
        ];
        apply_cache_breakpoints(&mut messages);
        assert!(marked(&messages[0]));
        // Second-to-last user turn: the first tool result, not the user task.
        assert!(!marked(&messages[1]));
        assert!(marked(&messages[3]));
        assert!(!marked(&messages[5]));
    }

    #[test]
    fn cache_breakpoints_without_system_mark_only_the_conversation() {
        let mut messages = vec![
            chat_message("user", "task"),
            chat_message("assistant", "answer"),
            chat_message("user", "follow-up"),
        ];
        apply_cache_breakpoints(&mut messages);
        assert!(marked(&messages[0]));
        assert!(!marked(&messages[2]));
    }
}
