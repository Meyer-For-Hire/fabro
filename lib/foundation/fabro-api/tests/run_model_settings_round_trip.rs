use std::any::{TypeId, type_name};
use std::collections::BTreeMap;

use fabro_api::types::{
    RunModelControls as ApiRunModelControls, RunModelSettings as ApiRunModelSettings,
};
use fabro_types::settings::run::{RunModelControls, RunModelSettings};

#[test]
fn run_model_settings_reuses_domain_types() {
    assert_same_type::<ApiRunModelSettings, RunModelSettings>();
    assert_same_type::<ApiRunModelControls, RunModelControls>();
}

#[test]
fn run_model_settings_json_matches_openapi_shape() {
    let settings = RunModelSettings {
        provider:  Some("openrouter".to_string()),
        name:      Some("claude-fable".to_string()),
        fallbacks: BTreeMap::from([("claude-fable".to_string(), vec![
            "gpt-sol".parse().expect("fixture reference should parse"),
            "openrouter:claude-opus"
                .parse()
                .expect("fixture reference should parse"),
        ])]),
        controls:  RunModelControls {
            reasoning_effort: Some("high".to_string()),
            speed:            None,
        },
    };

    let json = serde_json::to_value(&settings).expect("run model settings should serialize");
    assert_eq!(json["provider"], "openrouter");
    assert_eq!(json["name"], "claude-fable");
    assert_eq!(json["fallbacks"]["claude-fable"][0], "gpt-sol");
    assert_eq!(
        json["fallbacks"]["claude-fable"][1],
        "openrouter:claude-opus"
    );
    assert_eq!(json["controls"]["reasoning_effort"], "high");
    assert_eq!(json["controls"]["speed"], serde_json::Value::Null);

    let round_trip: ApiRunModelSettings =
        serde_json::from_value(json).expect("run model settings should deserialize");
    assert_eq!(round_trip, settings);
}

#[test]
fn run_model_settings_tolerates_absent_controls() {
    let parsed: RunModelSettings = serde_json::from_value(serde_json::json!({
        "provider": null,
        "name": null,
        "fallbacks": {}
    }))
    .expect("settings without controls should deserialize");
    assert_eq!(parsed.controls, RunModelControls::default());
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
