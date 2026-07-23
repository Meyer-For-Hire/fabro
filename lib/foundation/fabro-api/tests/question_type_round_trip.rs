use std::any::{TypeId, type_name};

use fabro_api::types::QuestionType as ApiQuestionType;
use fabro_types::QuestionType;
use serde_json::json;

#[test]
fn question_type_reuses_canonical_type() {
    assert_same_type::<ApiQuestionType, QuestionType>();
}

#[test]
fn question_type_serializes_as_snake_case_strings() {
    assert_eq!(
        serde_json::to_value(QuestionType::YesNo).unwrap(),
        json!("yes_no")
    );
    assert_eq!(
        serde_json::to_value(QuestionType::MultipleChoice).unwrap(),
        json!("multiple_choice")
    );
    assert_eq!(
        serde_json::to_value(QuestionType::MultiSelect).unwrap(),
        json!("multi_select")
    );
    assert_eq!(
        serde_json::to_value(QuestionType::Freeform).unwrap(),
        json!("freeform")
    );
    assert_eq!(
        serde_json::to_value(QuestionType::Confirmation).unwrap(),
        json!("confirmation")
    );
}

#[test]
fn question_type_deserializes_each_variant() {
    assert_eq!(
        serde_json::from_value::<QuestionType>(json!("yes_no")).unwrap(),
        QuestionType::YesNo
    );
    assert_eq!(
        serde_json::from_value::<QuestionType>(json!("multiple_choice")).unwrap(),
        QuestionType::MultipleChoice
    );
    assert_eq!(
        serde_json::from_value::<QuestionType>(json!("multi_select")).unwrap(),
        QuestionType::MultiSelect
    );
    assert_eq!(
        serde_json::from_value::<QuestionType>(json!("freeform")).unwrap(),
        QuestionType::Freeform
    );
    assert_eq!(
        serde_json::from_value::<QuestionType>(json!("confirmation")).unwrap(),
        QuestionType::Confirmation
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
