use std::path::Path;

use fabro_test::{fabro_snapshot, test_context};

use super::support::{output_stderr, setup_seeded_completed_dry_run};

#[expect(
    clippy::disallowed_methods,
    reason = "CLI integration test seeds a deterministic runtime log fixture with synchronous fs writes."
)]
fn seed_run_log(run_dir: &Path, contents: &[u8]) {
    // Raw logs are the CLI surface for this runtime-owned file; no public
    // command creates deterministic contents suitable for exact assertions.
    let log_path = run_dir.join("runtime/server.log");
    std::fs::create_dir_all(log_path.parent().expect("log path should have parent"))
        .expect("runtime log directory should be created");
    std::fs::write(&log_path, contents).expect("runtime log should be seeded");
}

#[test]
fn help() {
    let context = test_context!();
    let mut cmd = context.command();
    cmd.args(["logs", "--help"]);
    fabro_snapshot!(context.filters(), cmd, @r#"
    success: true
    exit_code: 0
    ----- stdout -----
    View the raw worker tracing log of a workflow run

    Usage: fabro logs [OPTIONS] <RUN>

    Arguments:
      <RUN>  Run ID prefix or workflow name (most recent run)

    Options:
          --json              Output as JSON [env: FABRO_JSON=]
          --server <SERVER>   Fabro server target: http(s) URL or absolute Unix socket path [env: FABRO_SERVER=]
          --debug             Enable DEBUG-level logging (default is INFO) [env: FABRO_DEBUG=]
      -n, --tail <TAIL>       Lines from end (default: all)
          --no-upgrade-check  Disable automatic upgrade check [env: FABRO_NO_UPGRADE_CHECK=true]
          --quiet             Suppress non-essential output [env: FABRO_QUIET=]
          --verbose           Enable verbose output [env: FABRO_VERBOSE=]
      -h, --help              Print help
    ----- stderr -----
    "#);
}

#[test]
fn logs_run_outputs_seeded_runtime_log_exactly() {
    let context = test_context!();
    let run = setup_seeded_completed_dry_run(&context);
    seed_run_log(&run.run_dir, b"worker started\nworker finished");

    let output = context
        .command()
        .args(["logs", &run.run_id])
        .output()
        .expect("command should run");

    assert!(
        output.status.success(),
        "logs should succeed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"worker started\nworker finished");
}

#[test]
fn logs_tail_one_outputs_final_line() {
    let context = test_context!();
    let run = setup_seeded_completed_dry_run(&context);
    seed_run_log(&run.run_dir, b"worker started\nworker finished");

    let output = context
        .command()
        .args(["logs", "--tail", "1", &run.run_id])
        .output()
        .expect("command should run");

    assert!(output.status.success());
    assert_eq!(output.stdout, b"worker finished\n");
}

#[test]
fn logs_tail_zero_outputs_nothing() {
    let context = test_context!();
    let run = setup_seeded_completed_dry_run(&context);
    seed_run_log(&run.run_dir, b"worker started\nworker finished");

    let output = context
        .command()
        .args(["logs", "--tail", "0", &run.run_id])
        .output()
        .expect("command should run");

    assert!(output.status.success());
    assert!(output.stdout.is_empty());
}

#[test]
fn logs_follow_is_unknown_argument() {
    let context = test_context!();
    let mut cmd = context.command();
    cmd.args(["logs", "--follow", "01K00000000000000000000000"]);

    fabro_snapshot!(context.filters(), cmd, @r#"
    success: false
    exit_code: 2
    ----- stdout -----
    ----- stderr -----
    error: unexpected argument '--follow' found

      tip: to pass '--follow' as a value, use '-- --follow'

    Usage: fabro logs [OPTIONS] <RUN>

    For more information, try '--help'.
    "#);
}

#[test]
fn logs_json_is_rejected() {
    let context = test_context!();
    let run = setup_seeded_completed_dry_run(&context);
    seed_run_log(&run.run_dir, b"worker started\n");

    let output = context
        .command()
        .args(["--json", "logs", &run.run_id])
        .output()
        .expect("command should run");

    assert!(!output.status.success());
    let stderr = output_stderr(&output);
    assert!(stderr.contains("--json is not supported for this command"));
}

#[test]
fn logs_missing_runtime_log_exits_nonzero() {
    let context = test_context!();
    let run = setup_seeded_completed_dry_run(&context);
    let _ = std::fs::remove_file(run.run_dir.join("runtime/server.log"));

    let output = context
        .command()
        .args(["logs", &run.run_id])
        .output()
        .expect("command should run");

    assert!(!output.status.success());
    let stderr = output_stderr(&output);
    assert!(stderr.contains("Run log not available"));
}
