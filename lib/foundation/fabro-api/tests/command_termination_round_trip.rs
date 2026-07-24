use std::any::{TypeId, type_name};

use fabro_api::types::CommandTermination as ApiCommandTermination;
use fabro_types::CommandTermination;
use serde_json::json;

#[test]
fn command_termination_reuses_canonical_type() {
    assert_same_type::<ApiCommandTermination, CommandTermination>();
}

#[test]
fn command_termination_serializes_as_state_names() {
    assert_eq!(
        serde_json::to_value(CommandTermination::Exited).unwrap(),
        json!("exited")
    );
    assert_eq!(
        serde_json::to_value(CommandTermination::TimedOut).unwrap(),
        json!("timed_out")
    );
    assert_eq!(
        serde_json::to_value(CommandTermination::Cancelled).unwrap(),
        json!("cancelled")
    );
}

#[test]
fn command_termination_deserializes_representative_values() {
    assert_eq!(
        serde_json::from_value::<ApiCommandTermination>(json!("exited")).unwrap(),
        CommandTermination::Exited
    );
    assert_eq!(
        serde_json::from_value::<ApiCommandTermination>(json!("timed_out")).unwrap(),
        CommandTermination::TimedOut
    );
    assert_eq!(
        serde_json::from_value::<ApiCommandTermination>(json!("cancelled")).unwrap(),
        CommandTermination::Cancelled
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
