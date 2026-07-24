use std::collections::HashSet;
use std::sync::Arc;

use fabro_model::{Catalog, ModelSelectionError};

use super::super::{
    ApiError, AppState, CompletionResponse, CompletionToolChoiceMode, CreateCompletionRequest,
    FinishReason, GenerateParams, IntoResponse, Json, LlmMessage, LlmRequest, ProviderId,
    RequiredUser, Response, Router, State, StatusCode, ToolChoice, ToolDefinition, Ulid, error,
    generate_object, info, post, warn,
};
use super::llm_sse;

pub(super) fn routes() -> Router<Arc<AppState>> {
    Router::new().route("/completions", post(create_completion))
}

fn finish_reason_to_api_stop_reason(reason: &FinishReason) -> String {
    match reason {
        FinishReason::Stop => "end_turn".to_string(),
        FinishReason::Length => "max_tokens".to_string(),
        FinishReason::ToolCalls => "tool_calls".to_string(),
        FinishReason::ContentFilter => "content_filter".to_string(),
        FinishReason::Error => "error".to_string(),
        FinishReason::Other(s) => s.clone(),
    }
}

async fn create_completion(
    _auth: RequiredUser,
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateCompletionRequest>,
) -> Response {
    let catalog = state.catalog();
    let llm_result = match state.resolve_llm_client().await {
        Ok(result) => result,
        Err(err) => {
            error!(error = ?err, "Failed to create LLM client");
            return ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to resolve LLM providers: {err}"),
            )
            .into_response();
        }
    };
    for (provider, issue) in &llm_result.auth_issues {
        warn!(provider = %provider, error = %issue, "LLM provider unavailable due to auth issue");
    }
    for issue in &llm_result.registration_issues {
        warn!(provider = %issue.provider, error = %issue.error, "LLM provider unavailable due to registration issue");
    }
    let client = llm_result.client;
    let (model_id, selected_provider) = match resolve_request_model(
        catalog.as_ref(),
        &client.provider_ids(),
        req.model.as_deref(),
        req.provider,
    ) {
        Ok(selection) => selection,
        Err(error) => return ApiError::bad_request(error.to_string()).into_response(),
    };

    // Build messages list. Request messages are already the canonical
    // `fabro_types::Message` — the API schema reuses it via build.rs
    // `with_replacement`, so no conversion is needed.
    let mut messages: Vec<LlmMessage> = Vec::new();
    if let Some(system) = req.system {
        messages.push(LlmMessage::system(system));
    }
    messages.extend(req.messages);

    // Convert tools
    let tools: Option<Vec<ToolDefinition>> = if req.tools.is_empty() {
        None
    } else {
        Some(
            req.tools
                .into_iter()
                .map(|t| ToolDefinition {
                    name:        t.name,
                    description: t.description,
                    parameters:  t.parameters,
                })
                .collect(),
        )
    };

    // Convert tool_choice
    let tool_choice: Option<ToolChoice> = req.tool_choice.map(|tc| match tc.mode {
        CompletionToolChoiceMode::Auto => ToolChoice::Auto,
        CompletionToolChoiceMode::None => ToolChoice::None,
        CompletionToolChoiceMode::Required => ToolChoice::Required,
        CompletionToolChoiceMode::Named => ToolChoice::named(tc.tool_name.unwrap_or_default()),
    });

    // Build the LLM request
    let request = LlmRequest {
        model: model_id.clone(),
        messages,
        provider: Some(selected_provider.to_string()),
        tools,
        tool_choice,
        response_format: None,
        temperature: req.temperature,
        top_p: req.top_p,
        max_tokens: req.max_tokens,
        stop_sequences: if req.stop_sequences.is_empty() {
            None
        } else {
            Some(req.stop_sequences)
        },
        reasoning_effort: req.reasoning_effort,
        speed: None,
        metadata: None,
        provider_options: req.provider_options,
    };
    info!(
        model = %model_id,
        provider = %selected_provider,
        "Completion request received"
    );

    // Force non-streaming for structured output
    let use_stream = req.stream && req.schema.is_none();

    if use_stream {
        // Streaming path: forward all StreamEvents as SSE
        let stream_result = match client.stream(&request).await {
            Ok(s) => s,
            Err(e) => {
                return ApiError::new(StatusCode::BAD_GATEWAY, format!("LLM error: {e}"))
                    .into_response();
            }
        };

        llm_sse::stream_response(stream_result, state.shutdown_token())
    } else {
        // Non-streaming path
        let msg_id = Ulid::new().to_string();

        if let Some(schema) = req.schema {
            // Structured output uses generate_object for JSON parsing logic.
            // tools/tool_choice are not forwarded: GenerateParams carries
            // executable Arc<Tool>s, not wire ToolDefinitions, and
            // generate_object sets response_format from the schema itself.
            let params = GenerateParams {
                messages: Some(request.messages),
                provider: request.provider,
                temperature: request.temperature,
                top_p: request.top_p,
                max_tokens: request.max_tokens,
                stop_sequences: request.stop_sequences,
                reasoning_effort: request.reasoning_effort,
                speed: request.speed,
                metadata: request.metadata,
                provider_options: request.provider_options,
                ..GenerateParams::new(request.model, std::sync::Arc::new(client.clone()))
            };
            match generate_object(params, schema).await {
                Ok(result) => {
                    // `result.finish_reason` / `result.usage` resolve through
                    // GenerateResult's Deref to the inner Response; move the
                    // Response out once so `message` can be taken by value.
                    let output = result.output;
                    let response = result.response;
                    let stop_reason = finish_reason_to_api_stop_reason(&response.finish_reason);
                    Json(CompletionResponse {
                        id: msg_id,
                        model: model_id,
                        provider: selected_provider,
                        message: response.message,
                        stop_reason,
                        usage: response.usage,
                        output,
                        cost_usd: response.cost_usd,
                        cost_source: response.cost_source,
                    })
                    .into_response()
                }
                Err(e) => ApiError::new(StatusCode::BAD_GATEWAY, format!("LLM error: {e}"))
                    .into_response(),
            }
        } else {
            match client.complete(&request).await {
                Ok(response) => {
                    let stop_reason = finish_reason_to_api_stop_reason(&response.finish_reason);
                    Json(CompletionResponse {
                        id: response.id,
                        model: response.model,
                        provider: ProviderId::new(response.provider),
                        message: response.message,
                        stop_reason,
                        usage: response.usage,
                        output: None,
                        cost_usd: response.cost_usd,
                        cost_source: response.cost_source,
                    })
                    .into_response()
                }
                Err(e) => ApiError::new(StatusCode::BAD_GATEWAY, format!("LLM error: {e}"))
                    .into_response(),
            }
        }
    }
}

pub(super) fn resolve_request_model(
    catalog: &Catalog,
    eligible: &HashSet<ProviderId>,
    requested_model: Option<&str>,
    explicit_provider: Option<String>,
) -> Result<(String, ProviderId), ModelSelectionError> {
    let explicit_provider = explicit_provider.map(ProviderId::new);
    let selected =
        catalog.resolve_selection(requested_model, explicit_provider.as_ref(), eligible)?;
    Ok((selected.model, selected.provider))
}
