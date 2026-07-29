use std::any::{TypeId, type_name};

use fabro_api::types::{
    InterviewQuestionRecord as ApiInterviewQuestionRecord, ReviewTarget as ApiReviewTarget,
    ReviewTargetKind as ApiReviewTargetKind,
};
use fabro_types::{InterviewQuestionRecord, ReviewTarget, ReviewTargetKind};
use serde_json::json;

#[test]
fn interview_question_record_reuses_canonical_type() {
    assert_same_type::<ApiInterviewQuestionRecord, InterviewQuestionRecord>();
    assert_same_type::<ApiReviewTarget, ReviewTarget>();
    assert_same_type::<ApiReviewTargetKind, ReviewTargetKind>();
}

#[test]
fn interview_question_record_round_trips_representative_json() {
    let value = json!({
        "id": "q-1",
        "text": "Approve deploy?",
        "stage": "gate",
        "question_type": "multiple_choice",
        "options": [
            {
                "key": "approve",
                "label": "Approve",
                "description": "Deploy now",
                "preview": "deploy --prod"
            },
            { "key": "reject", "label": "Reject" }
        ],
        "allow_freeform": true,
        "timeout_seconds": 30.0,
        "context_display": "Diff summary",
        "review_target": {
            "label": "Quarry review exercise",
            "url": "https://quarry.lithos.computer/tmp/0123456789abcdef0123456789abcdef",
            "kind": "document"
        }
    });

    let question: InterviewQuestionRecord = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(question).unwrap(), value);
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
