use fabro_api::types;
use serde_json::json;

#[test]
fn removed_docker_image_manifest_arg_is_ignored() {
    let args: types::ManifestArgs = serde_json::from_value(json!({
        "docker_image": "ghcr.io/fabro/custom:latest"
    }))
    .unwrap();

    assert_eq!(serde_json::to_value(args).unwrap(), json!({}));
}
