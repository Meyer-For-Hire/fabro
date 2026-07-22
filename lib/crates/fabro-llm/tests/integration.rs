#![expect(
    clippy::disallowed_methods,
    reason = "Live provider integration tests read required API keys from process env."
)]

use std::sync::Arc;

use fabro_auth::ApiCredential;
use fabro_llm::client::Client;
use fabro_llm::error::ProviderErrorKind;
use fabro_llm::model_test::{ModelTestStatus, run_model_test};
use fabro_llm::provider::ProviderAdapter;
use fabro_llm::providers::{
    AnthropicAdapter, BedrockAdapter, GeminiAdapter, OpenAiAdapter, OpenAiCompatibleAdapter,
};
use fabro_llm::types::{
    CostSource, FinishReason, Message, ReasoningEffort, Request, ToolChoice, ToolDefinition,
};
use fabro_model::catalog::{LlmCatalogSettings, ProviderCatalogSettings};
use fabro_model::{Catalog, ModelTestMode, ProviderId};
use fabro_static::EnvVars;

fn make_request(model: &str) -> Request {
    Request {
        model:            model.to_string(),
        messages:         vec![Message::user("Say hello in exactly one word")],
        provider:         None,
        tools:            None,
        tool_choice:      None,
        response_format:  None,
        temperature:      Some(0.0),
        top_p:            None,
        max_tokens:       Some(50),
        stop_sequences:   None,
        reasoning_effort: None,
        speed:            None,
        metadata:         None,
        provider_options: None,
    }
}

#[fabro_macros::e2e_test(live("ANTHROPIC_API_KEY"))]
async fn anthropic_complete() {
    let api_key = std::env::var(EnvVars::ANTHROPIC_API_KEY).expect("ANTHROPIC_API_KEY must be set");
    let adapter = AnthropicAdapter::new(api_key);
    let request = make_request("claude-haiku-4-5");
    let response = adapter.complete(&request).await.unwrap();

    assert!(
        !response.text().is_empty(),
        "response text should not be empty"
    );
    assert_eq!(response.finish_reason, FinishReason::Stop);
    assert!(response.usage.input_tokens > 0);
    assert!(response.usage.output_tokens > 0);
    assert_eq!(response.provider, "anthropic");
}

#[fabro_macros::e2e_test(twin, live("OPENAI_API_KEY"))]
async fn openai_complete() {
    let (base_url, api_key) = fabro_test::e2e_openai!();
    let adapter = OpenAiAdapter::new(api_key).with_base_url(base_url);
    let request = Request {
        temperature: None,
        ..make_request("gpt-5.2")
    };
    let response = adapter.complete(&request).await.unwrap();

    assert!(
        !response.text().is_empty(),
        "response text should not be empty"
    );
    assert_eq!(response.finish_reason, FinishReason::Stop);
    assert!(response.usage.input_tokens > 0);
    assert!(response.usage.output_tokens > 0);
    assert_eq!(response.provider, "openai");
}

#[fabro_macros::e2e_test(twin, live("OPENAI_API_KEY"))]
async fn openai_gpt_5_3_codex_complete() {
    let (base_url, api_key) = fabro_test::e2e_openai!();
    let adapter = OpenAiAdapter::new(api_key).with_base_url(base_url);
    let request = make_request("gpt-5.3-codex");
    let response = adapter.complete(&request).await.unwrap();

    assert!(
        !response.text().is_empty(),
        "response text should not be empty"
    );
    assert!(response.usage.input_tokens > 0);
    assert!(response.usage.output_tokens > 0);
    assert_eq!(response.provider, "openai");
}

#[fabro_macros::e2e_test(live("OPENAI_API_KEY"))]
async fn openai_gpt_5_5_complete() {
    let api_key = std::env::var(EnvVars::OPENAI_API_KEY).expect("OPENAI_API_KEY must be set");
    let adapter = OpenAiAdapter::new(api_key);
    let request = Request {
        temperature: None,
        ..make_request("gpt-5.5")
    };
    let response = adapter.complete(&request).await.unwrap();

    assert!(
        !response.text().is_empty(),
        "response text should not be empty"
    );
    assert!(response.usage.input_tokens > 0);
    assert!(response.usage.output_tokens > 0);
    assert_eq!(response.provider, "openai");
}

#[fabro_macros::e2e_test(live("OPENAI_GPT_5_5_PRO_API_KEY"))]
async fn openai_gpt_5_5_pro_complete() {
    let api_key = std::env::var("OPENAI_GPT_5_5_PRO_API_KEY")
        .expect("OPENAI_GPT_5_5_PRO_API_KEY must be set");
    let adapter = OpenAiAdapter::new(api_key);
    let request = Request {
        temperature: None,
        ..make_request("gpt-5.5-pro")
    };
    let response = adapter.complete(&request).await.unwrap();

    assert!(
        !response.text().is_empty(),
        "response text should not be empty"
    );
    assert!(response.usage.input_tokens > 0);
    assert!(response.usage.output_tokens > 0);
    assert_eq!(response.provider, "openai");
}

#[fabro_macros::e2e_test(live("KIMI_API_KEY"))]
async fn kimi_k3_reasoning_tool_round_trip() {
    let api_key = std::env::var(EnvVars::KIMI_API_KEY).expect("KIMI_API_KEY must be set");
    let adapter = OpenAiCompatibleAdapter::new(api_key, "https://api.moonshot.ai/v1")
        .with_name("kimi")
        .with_catalog(Arc::new(Catalog::from_builtin().unwrap()));
    let tool = ToolDefinition::function(
        "multiply",
        "Multiply two integers",
        serde_json::json!({
            "type": "object",
            "properties": {
                "a": {"type": "integer"},
                "b": {"type": "integer"}
            },
            "required": ["a", "b"]
        }),
    );
    let request = Request {
        model: "kimi-k3".to_string(),
        messages: vec![Message::user(
            "Use the multiply tool to calculate 19 times 23. Do not calculate it yourself.",
        )],
        tools: Some(vec![tool]),
        tool_choice: Some(ToolChoice::Required),
        temperature: Some(0.0),
        max_tokens: Some(4096),
        reasoning_effort: Some(ReasoningEffort::Low),
        ..make_request("kimi-k3")
    };

    let tool_response = adapter.complete(&request).await.unwrap();
    assert_eq!(tool_response.finish_reason, FinishReason::ToolCalls);
    assert!(
        tool_response.reasoning().is_some(),
        "K3 should return reasoning content before its tool call"
    );
    let tool_call = tool_response
        .tool_calls()
        .into_iter()
        .next()
        .expect("K3 should call the required tool");
    assert_eq!(tool_call.name, "multiply");

    let mut messages = request.messages.clone();
    messages.push(tool_response.message);
    messages.push(Message::tool_result(
        tool_call.id,
        serde_json::json!({"product": 437}),
        false,
    ));
    let final_request = Request {
        model: "kimi-k3".to_string(),
        messages,
        temperature: Some(0.0),
        max_tokens: Some(2048),
        reasoning_effort: Some(ReasoningEffort::Low),
        ..make_request("kimi-k3")
    };

    let final_response = adapter.complete(&final_request).await.unwrap();
    assert_eq!(final_response.finish_reason, FinishReason::Stop);
    assert!(
        final_response.text().contains("437"),
        "K3 should incorporate the replayed tool result"
    );
}

#[fabro_macros::e2e_test(twin)]
async fn openai_server_error() {
    let (base_url, api_key) = fabro_test::e2e_openai!();
    let admin_url = base_url
        .strip_suffix("/v1")
        .expect("OpenAI base URL should end with /v1");

    fabro_test::test_http_client()
        .post(format!("{admin_url}/__admin/scenarios"))
        .bearer_auth(&api_key)
        .json(&serde_json::json!({
            "scenarios": [{
                "matcher": { "endpoint": "responses" },
                "script": {
                    "kind": "error",
                    "status": 500,
                    "message": "internal server error",
                    "error_type": "server_error",
                    "code": "server_error"
                }
            }]
        }))
        .send()
        .await
        .unwrap();

    let adapter = OpenAiAdapter::new(api_key).with_base_url(base_url);
    let request = make_request("gpt-4o-mini");
    let err = adapter.complete(&request).await.unwrap_err();

    assert_eq!(err.provider_kind(), Some(ProviderErrorKind::Server));
    assert_eq!(err.status_code(), Some(500));
}

#[fabro_macros::e2e_test(live("GEMINI_API_KEY"))]
async fn gemini_complete() {
    let api_key = std::env::var(EnvVars::GEMINI_API_KEY).expect("GEMINI_API_KEY must be set");
    let adapter = GeminiAdapter::new(api_key);
    let request = make_request("gemini-2.5-flash");
    let response = adapter.complete(&request).await.unwrap();

    assert!(
        !response.text().is_empty(),
        "response text should not be empty"
    );
    assert_eq!(response.finish_reason, FinishReason::Stop);
    assert!(response.usage.input_tokens > 0);
    assert!(response.usage.output_tokens > 0);
    assert_eq!(response.provider, "gemini");
}

#[fabro_macros::e2e_test(live("AWS_BEARER_TOKEN_BEDROCK"))]
async fn bedrock_complete_with_api_key() {
    let token = std::env::var(EnvVars::AWS_BEARER_TOKEN_BEDROCK)
        .expect("AWS_BEARER_TOKEN_BEDROCK must be set");
    let adapter =
        BedrockAdapter::new_api_key(token, "https://bedrock-runtime.us-east-1.amazonaws.com")
            .unwrap()
            .with_name("bedrock");
    // Amazon Nova: first-party, no Anthropic-approval gate and no third-party
    // marketplace subscription, so this runs on any Bedrock-enabled account.
    let request = make_request("us.amazon.nova-2-lite-v1:0");
    let response = adapter.complete(&request).await.unwrap();

    assert!(
        !response.text().is_empty(),
        "response text should not be empty"
    );
    assert!(response.usage.input_tokens > 0);
    assert!(response.usage.output_tokens > 0);
    assert_eq!(response.provider, "bedrock");
}

#[fabro_macros::e2e_test(live("AWS_ACCESS_KEY_ID"))]
async fn bedrock_complete_with_sigv4() {
    let adapter = BedrockAdapter::new_sigv4("https://bedrock-runtime.us-east-1.amazonaws.com")
        .unwrap()
        .with_name("bedrock");
    // First-party Nova — see bedrock_complete_with_api_key for why.
    let request = make_request("us.amazon.nova-2-lite-v1:0");
    let response = adapter.complete(&request).await.unwrap();

    assert!(
        !response.text().is_empty(),
        "response text should not be empty"
    );
    assert!(response.usage.input_tokens > 0);
    assert_eq!(response.provider, "bedrock");
}

#[fabro_macros::e2e_test(live("AWS_BEARER_TOKEN_BEDROCK"))]
async fn bedrock_openai_frontier_complete() {
    let token = std::env::var(EnvVars::AWS_BEARER_TOKEN_BEDROCK)
        .expect("AWS_BEARER_TOKEN_BEDROCK must be set");
    // GPT-5.x on Bedrock is the bedrock-mantle Responses surface: the plain
    // openai adapter pointed at the mantle endpoint with the Bedrock key as
    // the bearer token.
    let adapter = OpenAiAdapter::new(token)
        .with_base_url("https://bedrock-mantle.us-east-1.api.aws/openai/v1")
        .with_name("bedrock-openai");
    let request = Request {
        temperature: None,
        ..make_request("openai.gpt-5.5")
    };
    let response = adapter.complete(&request).await.unwrap();

    assert!(
        !response.text().is_empty(),
        "response text should not be empty"
    );
    assert!(response.usage.input_tokens > 0);
    assert_eq!(response.provider, "bedrock-openai");
}

#[fabro_macros::e2e_test(live("OPENROUTER_API_KEY"))]
async fn openrouter_complete() {
    let api_key =
        std::env::var(EnvVars::OPENROUTER_API_KEY).expect("OPENROUTER_API_KEY must be set");
    let adapter = OpenAiCompatibleAdapter::new(api_key, "https://openrouter.ai/api/v1")
        .with_name("openrouter");
    let request = make_request("deepseek/deepseek-v4-flash");
    let response = adapter.complete(&request).await.unwrap();

    assert!(
        !response.text().is_empty(),
        "response text should not be empty"
    );
    assert!(response.usage.input_tokens > 0);
    assert!(response.usage.output_tokens > 0);
    assert_eq!(response.provider, "openrouter");
    assert!(
        response.cost_usd.is_some(),
        "OpenRouter responses should carry an authoritative usage.cost",
    );
    assert_eq!(response.cost_source, Some(CostSource::Authoritative));
}

#[fabro_macros::e2e_test(live("OPENROUTER_API_KEY"))]
async fn openrouter_kimi_k3_deep_tool_round_trip() {
    let api_key =
        std::env::var(EnvVars::OPENROUTER_API_KEY).expect("OPENROUTER_API_KEY must be set");
    let provider = ProviderId::new("openrouter");
    let mut settings = LlmCatalogSettings::default();
    settings
        .providers
        .insert(provider.to_string(), ProviderCatalogSettings {
            enabled: Some(true),
            ..ProviderCatalogSettings::default()
        });
    let catalog = Arc::new(
        Catalog::from_builtin_with_overrides(&settings)
            .expect("enabled OpenRouter catalog should build"),
    );
    let credential = ApiCredential::from_api_key(provider, api_key, &catalog)
        .expect("OpenRouter credential should resolve from the catalog");
    let client = Arc::new(
        Client::from_credentials(vec![credential], Arc::clone(&catalog))
            .await
            .expect("OpenRouter client should build from the catalog"),
    );
    let model = catalog
        .get("moonshotai/kimi-k3")
        .expect("OpenRouter Kimi K3 should be present");

    let outcome = run_model_test(model, ModelTestMode::Deep, client).await;
    assert_eq!(
        outcome.status,
        ModelTestStatus::Ok,
        "OpenRouter Kimi K3 deep test failed: {:?}",
        outcome.error_message
    );
}

async fn run_multi_turn_cache_test(
    adapter: &dyn ProviderAdapter,
    model: &str,
    min_cache_ratio: f64,
    temperature: Option<f64>,
) {
    // Claude Haiku 4.5 requires 4096 tokens minimum for prompt caching.
    // Each repeat is ~78 tokens; 70 repeats ≈ 5460 tokens, safely above the
    // threshold.
    let padding = "This is a detailed context paragraph that provides background information \
        about the conversation. It contains various facts and details that the model should \
        remember throughout the multi-turn interaction. The purpose of this padding is to \
        ensure the system prompt exceeds the minimum cache threshold for the provider. \
        We include information about mathematics, science, history, and general knowledge. \
        The model should use this context when answering questions. "
        .repeat(70);

    let system_message = Message::system(format!(
        "You are a helpful math assistant. Answer briefly.\n\n{padding}"
    ));

    let questions = [
        "What is 1+1?",
        "What is 2+2?",
        "What is 3+3?",
        "What is 4+4?",
        "What is 5+5?",
        "What is 6+6?",
    ];

    let mut messages = vec![system_message, Message::user(questions[0])];
    let mut best_cache_ratio = 0.0_f64;

    for turn in 0..6 {
        let request = Request {
            model: model.to_string(),
            messages: messages.clone(),
            provider: None,
            tools: None,
            tool_choice: None,
            response_format: None,
            temperature,
            top_p: None,
            max_tokens: Some(100),
            stop_sequences: None,
            reasoning_effort: None,
            speed: None,
            metadata: None,
            provider_options: None,
        };

        let response = adapter
            .complete(&request)
            .await
            .expect("provider adapter should return a response");
        let text = response.text();
        assert!(
            !text.is_empty(),
            "response text should not be empty on turn {turn}"
        );

        let cache_read = response.usage.cache_read_tokens as f64;
        let input = response.usage.input_tokens as f64;
        let ratio = cache_read / input;
        best_cache_ratio = best_cache_ratio.max(ratio);

        messages.push(Message::assistant(text));
        if turn < 5 {
            messages.push(Message::user(questions[turn + 1]));
        }
    }

    assert!(
        best_cache_ratio >= min_cache_ratio,
        "best cache ratio {best_cache_ratio:.3} should be at least {min_cache_ratio} across all turns"
    );
}

#[fabro_macros::e2e_test(live("ANTHROPIC_API_KEY"))]
async fn anthropic_multi_turn_cache() {
    let api_key = std::env::var(EnvVars::ANTHROPIC_API_KEY).expect("ANTHROPIC_API_KEY must be set");
    let adapter =
        AnthropicAdapter::new(api_key).with_catalog(Arc::new(Catalog::from_builtin().unwrap()));
    run_multi_turn_cache_test(&adapter, "claude-haiku-4-5", 0.5, Some(0.0)).await;
}

#[fabro_macros::e2e_test(live("OPENAI_API_KEY"))]
async fn openai_multi_turn_cache() {
    let api_key = std::env::var(EnvVars::OPENAI_API_KEY).expect("OPENAI_API_KEY must be set");
    let adapter = OpenAiAdapter::new(api_key);
    run_multi_turn_cache_test(&adapter, "gpt-5.2", 0.5, None).await;
}

#[fabro_macros::e2e_test(live("GEMINI_API_KEY"))]
async fn gemini_multi_turn_cache() {
    let api_key = std::env::var(EnvVars::GEMINI_API_KEY).expect("GEMINI_API_KEY must be set");
    let adapter = GeminiAdapter::new(api_key);
    run_multi_turn_cache_test(&adapter, "gemini-2.5-flash", 0.5, Some(0.0)).await;
}
