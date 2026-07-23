use std::any::{TypeId, type_name};

use fabro_api::types::StageOutcome as ApiStageOutcome;
use fabro_types::StageOutcome;
use serde_json::json;

#[test]
fn stage_outcome_reuses_canonical_type() {
    assert_same_type::<ApiStageOutcome, StageOutcome>();
}

#[test]
fn stage_outcome_serializes_as_terminal_strings() {
    assert_eq!(
        serde_json::to_value(StageOutcome::Succeeded).unwrap(),
        json!("succeeded")
    );
    assert_eq!(
        serde_json::to_value(StageOutcome::PartiallySucceeded).unwrap(),
        json!("partially_succeeded")
    );
    assert_eq!(
        serde_json::to_value(StageOutcome::Failed {
            retry_requested: false,
        })
        .unwrap(),
        json!("failed")
    );
    assert_eq!(
        serde_json::to_value(StageOutcome::Failed {
            retry_requested: true,
        })
        .unwrap(),
        json!("failed")
    );
    assert_eq!(
        serde_json::to_value(StageOutcome::Skipped).unwrap(),
        json!("skipped")
    );
}

#[test]
fn stage_outcome_failed_wire_form_is_lossy() {
    assert_eq!(
        serde_json::from_value::<ApiStageOutcome>(json!("failed")).unwrap(),
        StageOutcome::Failed {
            retry_requested: false,
        }
    );

    let encoded = serde_json::to_string(&StageOutcome::Failed {
        retry_requested: true,
    })
    .unwrap();
    assert_eq!(encoded, "\"failed\"");
    assert_eq!(
        serde_json::from_str::<ApiStageOutcome>(&encoded).unwrap(),
        StageOutcome::Failed {
            retry_requested: false,
        }
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
