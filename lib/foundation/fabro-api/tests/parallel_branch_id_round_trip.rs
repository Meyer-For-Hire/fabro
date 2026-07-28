use std::any::{TypeId, type_name};

use fabro_api::types::ParallelBranchId as ApiParallelBranchId;
use fabro_types::{ParallelBranchId, StageId};
use serde_json::json;

#[test]
fn parallel_branch_id_reuses_canonical_type() {
    assert_same_type::<ApiParallelBranchId, ParallelBranchId>();
}

#[test]
fn parallel_branch_id_round_trips_openapi_representation() {
    let branch_id = ParallelBranchId::new(StageId::new("review_fork", 3), 1);

    assert_eq!(
        serde_json::to_value(&branch_id).unwrap(),
        json!("review_fork@3:1")
    );
    assert_eq!(
        serde_json::from_value::<ApiParallelBranchId>(json!("review_fork@3:1")).unwrap(),
        branch_id
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
