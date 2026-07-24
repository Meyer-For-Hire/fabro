use std::any::{TypeId, type_name};

use fabro_api::types::CompletionUsage as ApiCompletionUsage;
use fabro_model::TokenCounts;
use serde_json::json;

#[test]
fn completion_usage_reuses_canonical_type() {
    assert_same_type::<ApiCompletionUsage, TokenCounts>();
}

#[test]
fn completion_usage_json_matches_openapi_shape() {
    let usage = TokenCounts {
        input_tokens:       10,
        output_tokens:      20,
        reasoning_tokens:   3,
        cache_read_tokens:  4,
        cache_write_tokens: 5,
    };

    let json = serde_json::to_value(&usage).unwrap();
    assert_eq!(json["input_tokens"], 10);
    assert_eq!(json["output_tokens"], 20);
    assert_eq!(json["reasoning_tokens"], 3);
    assert_eq!(json["cache_read_tokens"], 4);
    assert_eq!(json["cache_write_tokens"], 5);

    let round_trip: ApiCompletionUsage = serde_json::from_value(json).unwrap();
    assert_eq!(round_trip, usage);
}

#[test]
fn completion_usage_keeps_zero_counts_present() {
    let json = serde_json::to_value(TokenCounts::default()).unwrap();
    assert_eq!(
        json,
        json!({
            "input_tokens": 0,
            "output_tokens": 0,
            "reasoning_tokens": 0,
            "cache_read_tokens": 0,
            "cache_write_tokens": 0
        })
    );

    let round_trip: ApiCompletionUsage = serde_json::from_value(json).unwrap();
    assert_eq!(round_trip, TokenCounts::default());
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
