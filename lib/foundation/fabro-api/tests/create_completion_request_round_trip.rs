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

    let reasoning_effort: Option<ReasoningEffort> = request.reasoning_effort;
    assert_eq!(reasoning_effort, Some(ReasoningEffort::High));
}

#[test]
fn create_completion_request_rejects_unknown_reasoning_effort() {
    let result = serde_json::from_value::<CreateCompletionRequest>(json!({
        "messages": [],
        "reasoning_effort": "bogus"
    }));

    assert!(result.is_err());
}
