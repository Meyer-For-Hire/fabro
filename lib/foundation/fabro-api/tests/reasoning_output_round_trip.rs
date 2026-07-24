use std::any::{TypeId, type_name};

use fabro_api::types::{
    AgentMessageProps as ApiAgentMessageProps, ReasoningOutput as ApiReasoningOutput,
};
use fabro_types::ReasoningOutput;
use serde_json::json;

#[test]
fn reasoning_output_reuses_canonical_type() {
    assert_same_type::<ApiReasoningOutput, ReasoningOutput>();
}

#[test]
fn reasoning_output_matches_openapi_json_shape() {
    let value = json!({
        "summary": "inspect the conversion first",
        "trace": "read convert.rs, then the sink",
    });

    let output: ReasoningOutput = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(
        output.summary.as_deref(),
        Some("inspect the conversion first")
    );
    assert_eq!(
        output.trace.as_deref(),
        Some("read convert.rs, then the sink")
    );
    assert_eq!(serde_json::to_value(&output).unwrap(), value);

    let api_output: ApiReasoningOutput = serde_json::from_value(value).unwrap();
    assert_eq!(api_output, output);
}

#[test]
fn reasoning_output_members_are_individually_optional() {
    let summary_only: ReasoningOutput =
        serde_json::from_value(json!({"summary": "only a summary"})).unwrap();
    assert!(summary_only.trace.is_none());
    assert_eq!(
        serde_json::to_value(&summary_only).unwrap(),
        json!({"summary": "only a summary"})
    );

    let trace_only: ReasoningOutput =
        serde_json::from_value(json!({"trace": "only a trace"})).unwrap();
    assert!(trace_only.summary.is_none());
    assert_eq!(
        serde_json::to_value(&trace_only).unwrap(),
        json!({"trace": "only a trace"})
    );
}

#[test]
fn agent_message_props_reasoning_is_optional_on_the_wire() {
    let without = json!({
        "text": "ok",
        "model": {"provider": "openai", "model_id": "gpt-5.4"},
        "billing": {"input_tokens": 1, "output_tokens": 1, "total_tokens": 2},
        "tool_call_count": 0,
        "visit": 1,
    });
    let props: ApiAgentMessageProps = serde_json::from_value(without.clone()).unwrap();
    assert!(props.reasoning.is_none());
    assert!(
        !serde_json::to_value(&props)
            .unwrap()
            .as_object()
            .unwrap()
            .contains_key("reasoning")
    );

    let mut with = without;
    with["reasoning"] = json!({"summary": "checked the parser", "trace": "step one"});
    let props: ApiAgentMessageProps = serde_json::from_value(with).unwrap();
    let reasoning = props.reasoning.as_ref().unwrap();
    assert_eq!(reasoning.summary.as_deref(), Some("checked the parser"));
    assert_eq!(reasoning.trace.as_deref(), Some("step one"));
    assert_eq!(
        serde_json::to_value(&props).unwrap()["reasoning"],
        json!({"summary": "checked the parser", "trace": "step one"})
    );
}

fn assert_same_type<T: 'static, U: 'static>() {
    assert_eq!(
        TypeId::of::<T>(),
        TypeId::of::<U>(),
        "{} should be the same type as {}",
        type_name::<T>(),
        type_name::<U>()
    );
}
