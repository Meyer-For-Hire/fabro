use fabro_test::{fabro_snapshot, test_context};
use httpmock::MockServer;
use serde_json::Value;

#[test]
fn help() {
    let context = test_context!();
    let mut cmd = context.command();
    cmd.args(["system", "repair", "runs", "--help"]);
    fabro_snapshot!(context.filters(), cmd, @"
    success: true
    exit_code: 0
    ----- stdout -----
    List runs that cannot be loaded from durable storage

    Usage: fabro system repair runs [OPTIONS]

    Options:
          --json                       Output as JSON [env: FABRO_JSON=]
          --storage-dir <STORAGE_DIR>  Local storage directory (default: ~/.fabro/storage) [env: FABRO_STORAGE_DIR=]
          --debug                      Enable DEBUG-level logging (default is INFO) [env: FABRO_DEBUG=]
          --server <SERVER>            Fabro server target: http(s) URL or absolute Unix socket path [env: FABRO_SERVER=]
          --delete                     Preview deleting unreadable runs
          --no-upgrade-check           Disable automatic upgrade check [env: FABRO_NO_UPGRADE_CHECK=true]
          --quiet                      Suppress non-essential output [env: FABRO_QUIET=]
          --yes                        Actually delete unreadable runs (default is dry-run)
          --verbose                    Enable verbose output [env: FABRO_VERBOSE=]
      -h, --help                       Print help
    ----- stderr -----
    ");
}

#[test]
fn system_repair_runs_reports_unreadable_runs() {
    let context = test_context!();
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method("GET").path("/api/v1/system/repair/runs");
        then.status(200)
            .header("Content-Type", "application/json")
            .body(
                serde_json::json!({
                    "runs": [{
                        "run_id": "01KQT1TNZ0QXK0QHP10G0V5X84",
                        "created_at": "2026-05-05T20:46:33Z",
                        "error": "Serialization error: missing field `integrations`",
                    }],
                    "total_count": 1,
                })
                .to_string(),
            );
    });
    context.set_http_target(&server.base_url());

    let output = context
        .command()
        .args(["system", "repair", "runs"])
        .output()
        .expect("command should run");

    assert!(output.status.success(), "system repair runs failed");
    let stdout = String::from_utf8(output.stdout).expect("stdout should be valid UTF-8");
    assert!(stdout.contains("Unreadable runs:"), "{stdout}");
    assert!(stdout.contains("01KQT1TNZ0QXK0QHP10G0V5X84"), "{stdout}");
    assert!(stdout.contains("missing field `integrations`"), "{stdout}");
    assert!(
        stdout.contains("fabro rm --force 01KQT1TNZ0QXK0QHP10G0V5X84"),
        "{stdout}"
    );
    mock.assert();
}

#[test]
fn system_repair_runs_delete_previews_deletion() {
    let context = test_context!();
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method("GET").path("/api/v1/system/repair/runs");
        then.status(200)
            .header("Content-Type", "application/json")
            .body(repair_runs_body(["01KQT1TNZ0QXK0QHP10G0V5X84"]));
    });
    context.set_http_target(&server.base_url());

    let output = context
        .command()
        .args(["system", "repair", "runs", "--delete"])
        .output()
        .expect("command should run");

    assert!(output.status.success(), "system repair runs failed");
    let stdout = String::from_utf8(output.stdout).expect("stdout should be valid UTF-8");
    assert!(stdout.contains("Unreadable runs:"), "{stdout}");
    assert!(
        stdout.contains("1 unreadable run(s) would be deleted. Pass --delete --yes to confirm."),
        "{stdout}"
    );
    mock.assert();
}

#[test]
fn system_repair_runs_delete_yes_forces_every_delete() {
    let context = test_context!();
    let server = MockServer::start();
    let get_mock = server.mock(|when, then| {
        when.method("GET").path("/api/v1/system/repair/runs");
        then.status(200)
            .header("Content-Type", "application/json")
            .body(repair_runs_body([
                "01KQT1TNZ0QXK0QHP10G0V5X84",
                "01KQT1TZ5BKJ9C1GY9WY6AVZ6Y",
            ]));
    });
    let first_delete = server.mock(|when, then| {
        when.method("DELETE")
            .path("/api/v1/runs/01KQT1TNZ0QXK0QHP10G0V5X84")
            .query_param("force", "true");
        then.status(204);
    });
    let second_delete = server.mock(|when, then| {
        when.method("DELETE")
            .path("/api/v1/runs/01KQT1TZ5BKJ9C1GY9WY6AVZ6Y")
            .query_param("force", "true");
        then.status(204);
    });
    context.set_http_target(&server.base_url());

    let output = context
        .command()
        .args(["system", "repair", "runs", "--delete", "--yes"])
        .output()
        .expect("command should run");

    assert!(
        output.status.success(),
        "system repair runs failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout should be valid UTF-8");
    assert!(
        stdout.contains("deleted: 01KQT1TNZ0QXK0QHP10G0V5X84"),
        "{stdout}"
    );
    assert!(
        stdout.contains("deleted: 01KQT1TZ5BKJ9C1GY9WY6AVZ6Y"),
        "{stdout}"
    );
    get_mock.assert();
    first_delete.assert();
    second_delete.assert();
}

#[test]
fn system_repair_runs_delete_yes_reports_partial_failures_after_attempting_all() {
    let context = test_context!();
    let server = MockServer::start();
    let get_mock = server.mock(|when, then| {
        when.method("GET").path("/api/v1/system/repair/runs");
        then.status(200)
            .header("Content-Type", "application/json")
            .body(repair_runs_body([
                "01KQT1TNZ0QXK0QHP10G0V5X84",
                "01KQT1TZ5BKJ9C1GY9WY6AVZ6Y",
            ]));
    });
    let failed_delete = server.mock(|when, then| {
        when.method("DELETE")
            .path("/api/v1/runs/01KQT1TNZ0QXK0QHP10G0V5X84")
            .query_param("force", "true");
        then.status(500)
            .header("Content-Type", "application/json")
            .body(
                serde_json::json!({
                    "errors": [{ "detail": "delete failed" }]
                })
                .to_string(),
            );
    });
    let successful_delete = server.mock(|when, then| {
        when.method("DELETE")
            .path("/api/v1/runs/01KQT1TZ5BKJ9C1GY9WY6AVZ6Y")
            .query_param("force", "true");
        then.status(204);
    });
    context.set_http_target(&server.base_url());

    let output = context
        .command()
        .args(["system", "repair", "runs", "--delete", "--yes"])
        .output()
        .expect("command should run");

    assert!(
        !output.status.success(),
        "partial failure should exit nonzero"
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout should be valid UTF-8");
    let stderr = String::from_utf8(output.stderr).expect("stderr should be valid UTF-8");
    assert!(
        stdout.contains("deleted: 01KQT1TZ5BKJ9C1GY9WY6AVZ6Y"),
        "{stdout}"
    );
    assert!(
        stderr.contains("error: 01KQT1TNZ0QXK0QHP10G0V5X84"),
        "{stderr}"
    );
    assert!(
        stderr.contains("some unreadable runs could not be deleted"),
        "{stderr}"
    );
    get_mock.assert();
    failed_delete.assert();
    successful_delete.assert();
}

#[test]
fn system_repair_runs_delete_json_emits_summary() {
    let context = test_context!();
    let server = MockServer::start();
    let get_mock = server.mock(|when, then| {
        when.method("GET").path("/api/v1/system/repair/runs");
        then.status(200)
            .header("Content-Type", "application/json")
            .body(repair_runs_body(["01KQT1TNZ0QXK0QHP10G0V5X84"]));
    });
    let delete_mock = server.mock(|when, then| {
        when.method("DELETE")
            .path("/api/v1/runs/01KQT1TNZ0QXK0QHP10G0V5X84")
            .query_param("force", "true");
        then.status(204);
    });
    context.set_http_target(&server.base_url());

    let output = context
        .command()
        .args(["--json", "system", "repair", "runs", "--delete", "--yes"])
        .output()
        .expect("command should run");

    assert!(output.status.success());
    let value: Value =
        serde_json::from_slice(&output.stdout).expect("system repair JSON should parse");
    assert_eq!(value["dry_run"], false);
    assert_eq!(value["total_count"], 1);
    assert_eq!(value["runs"][0]["run_id"], "01KQT1TNZ0QXK0QHP10G0V5X84");
    assert_eq!(value["deleted"][0], "01KQT1TNZ0QXK0QHP10G0V5X84");
    assert!(value["errors"].as_array().unwrap().is_empty());
    get_mock.assert();
    delete_mock.assert();
}

#[test]
fn system_repair_runs_json_emits_api_response() {
    let context = test_context!();
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method("GET").path("/api/v1/system/repair/runs");
        then.status(200)
            .header("Content-Type", "application/json")
            .body(
                serde_json::json!({
                    "runs": [{
                        "run_id": "01KQT1TNZ0QXK0QHP10G0V5X84",
                        "created_at": "2026-05-05T20:46:33Z",
                        "error": "Serialization error: missing field `integrations`",
                    }],
                    "total_count": 1,
                })
                .to_string(),
            );
    });
    context.set_http_target(&server.base_url());

    let output = context
        .command()
        .args(["--json", "system", "repair", "runs"])
        .output()
        .expect("command should run");

    assert!(output.status.success());
    let value: Value =
        serde_json::from_slice(&output.stdout).expect("system repair JSON should parse");
    assert_eq!(value["total_count"], 1);
    assert_eq!(value["runs"][0]["run_id"], "01KQT1TNZ0QXK0QHP10G0V5X84");
    mock.assert();
}

#[test]
fn system_repair_runs_reports_empty_state() {
    let context = test_context!();
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method("GET").path("/api/v1/system/repair/runs");
        then.status(200)
            .header("Content-Type", "application/json")
            .body(serde_json::json!({ "runs": [], "total_count": 0 }).to_string());
    });
    context.set_http_target(&server.base_url());

    let output = context
        .command()
        .args(["system", "repair", "runs"])
        .output()
        .expect("command should run");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("stdout should be valid UTF-8");
    assert!(stdout.contains("No run repair issues found."), "{stdout}");
    mock.assert();
}

fn repair_runs_body<const N: usize>(run_ids: [&str; N]) -> String {
    serde_json::json!({
        "runs": run_ids
            .into_iter()
            .map(|run_id| serde_json::json!({
                "run_id": run_id,
                "created_at": "2026-05-05T20:46:33Z",
                "error": "Serialization error: missing field `integrations`",
            }))
            .collect::<Vec<_>>(),
        "total_count": N,
    })
    .to_string()
}
