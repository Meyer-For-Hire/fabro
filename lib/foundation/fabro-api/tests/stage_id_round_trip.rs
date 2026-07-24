use std::any::{TypeId, type_name};

use fabro_api::types::StageId as ApiStageId;
use fabro_types::StageId;
use serde_json::json;

#[test]
fn stage_id_reuses_canonical_type() {
    assert_same_type::<ApiStageId, StageId>();
}

#[test]
fn stage_id_round_trips_openapi_representation() {
    let stage_id = StageId::new("verify", 2);

    assert_eq!(serde_json::to_value(&stage_id).unwrap(), json!("verify@2"));
    assert_eq!(
        serde_json::from_value::<ApiStageId>(json!("verify@2")).unwrap(),
        stage_id
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
