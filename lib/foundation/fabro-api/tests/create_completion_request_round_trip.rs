use fabro_api::types::CreateCompletionRequest;
use fabro_model::ReasoningEffort;
use serde_json::json;

#[test]
fn create_completion_request_reuses_canonical_reasoning_effort() {
    let request: CreateCompletionRequest = serde_json::from_value(json!({
        "messages": [],
        "reasoning_effort": "high"
    }))
    .unwrap();

    assert_eq!(request.reasoning_effort, Some(ReasoningEffort::High));
}
