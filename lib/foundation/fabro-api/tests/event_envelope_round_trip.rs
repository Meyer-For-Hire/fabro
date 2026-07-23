use std::any::{TypeId, type_name};

use fabro_api::types::EventEnvelope as ApiEventEnvelope;
use fabro_types::{EventEnvelope, fixtures};
use serde_json::json;

#[test]
fn event_envelope_reuses_canonical_type() {
    assert_same_type::<ApiEventEnvelope, EventEnvelope>();
}

#[test]
fn event_envelope_round_trips_flattened_run_event_json() {
    let value = json!({
        "seq": 42,
        "id": "evt_envelope",
        "ts": "2026-04-29T12:03:00Z",
        "run_id": fixtures::RUN_1,
        "event": "stage.started",
        "node_id": "code",
        "node_label": "Code",
        "stage_id": "code@2",
        "properties": {
            "index": 1,
            "handler_type": "agent",
            "attempt": 2,
            "max_attempts": 3
        }
    });

    let envelope: EventEnvelope = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(envelope).unwrap(), value);
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
