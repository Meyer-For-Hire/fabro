use fabro_test::test_context;

#[test]
fn emits_cli_reference_markdown() {
    let context = test_context!();
    let output = context
        .command()
        .arg("__cli-reference")
        .assert()
        .success()
        .get_output()
        .clone();
    let stdout = String::from_utf8(output.stdout).expect("stdout should be valid UTF-8");

    assert!(
        stdout.contains("## `fabro`"),
        "CLI reference should include the root command:\n{stdout}"
    );
    assert!(
        stdout.contains("### `fabro run`"),
        "CLI reference should include visible subcommands:\n{stdout}"
    );
    assert!(
        !stdout.contains("__cli-reference"),
        "CLI reference should not include hidden commands:\n{stdout}"
    );
    assert!(
        output.stderr.is_empty(),
        "CLI reference command should not emit stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
