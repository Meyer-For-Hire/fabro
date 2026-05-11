use std::collections::BTreeMap;

use chrono::{TimeZone, Utc};
use fabro_types::{
    RunSandbox, RunSandboxRuntime, SandboxDetails, SandboxProvider, SandboxResources, SandboxState,
    SandboxTimestamps,
};
use serde_json::json;

#[test]
fn run_sandbox_serializes_canonical_identity_without_identifier() {
    let sandbox = RunSandbox {
        provider: SandboxProvider::Docker,
        image:    None,
        snapshot: None,
        runtime:  Some(RunSandboxRuntime {
            id:                "container-abc123".to_string(),
            working_directory: "/workspace".to_string(),
            repo_cloned:       Some(true),
            clone_origin_url:  Some("https://github.com/fabro-sh/fabro.git".to_string()),
            clone_branch:      Some("main".to_string()),
        }),
    };

    let value = serde_json::to_value(&sandbox).unwrap();

    assert_eq!(
        value,
        json!({
            "provider": "docker",
            "runtime": {
                "id": "container-abc123",
                "working_directory": "/workspace",
                "repo_cloned": true,
                "clone_origin_url": "https://github.com/fabro-sh/fabro.git",
                "clone_branch": "main"
            }
        })
    );
    assert!(value.get("identifier").is_none());
}

#[test]
fn sandbox_details_requires_canonical_id_and_working_directory() {
    let details = SandboxDetails {
        sandbox:      RunSandbox {
            provider: SandboxProvider::Daytona,
            image:    Some("ubuntu:24.04".to_string()),
            snapshot: None,
            runtime:  Some(RunSandboxRuntime {
                id:                "daytona-sandbox-name".to_string(),
                working_directory: "/workspace".to_string(),
                repo_cloned:       None,
                clone_origin_url:  None,
                clone_branch:      None,
            }),
        },
        state:        SandboxState::Running,
        native_state: Some("started".to_string()),
        region:       Some("us".to_string()),
        resources:    SandboxResources {
            cpu_cores:    Some(2.0),
            memory_bytes: Some(4 * 1024 * 1024 * 1024),
            disk_bytes:   None,
        },
        labels:       BTreeMap::from([("run".to_string(), "abc".to_string())]),
        timestamps:   SandboxTimestamps {
            created_at:       Some(Utc.with_ymd_and_hms(2026, 5, 9, 12, 0, 0).unwrap()),
            last_activity_at: None,
        },
    };

    let value = serde_json::to_value(&details).unwrap();

    assert_eq!(value["sandbox"]["provider"], "daytona");
    assert_eq!(value["sandbox"]["runtime"]["id"], "daytona-sandbox-name");
    assert_eq!(
        value["sandbox"]["runtime"]["working_directory"],
        "/workspace"
    );
    assert!(value.get("name").is_none());
    assert!(value.get("identifier").is_none());
}

#[test]
fn sandbox_provider_rejects_unknown_values() {
    assert_eq!(
        serde_json::from_value::<SandboxProvider>(json!("local")).unwrap(),
        SandboxProvider::Local
    );
    assert_eq!(
        serde_json::from_value::<SandboxProvider>(json!("docker")).unwrap(),
        SandboxProvider::Docker
    );
    assert_eq!(
        serde_json::from_value::<SandboxProvider>(json!("daytona")).unwrap(),
        SandboxProvider::Daytona
    );
    assert!(serde_json::from_value::<SandboxProvider>(json!("other")).is_err());
}
