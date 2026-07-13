use std::process::Command;

#[test]
fn test_help_output_does_not_leak_token() {
    let output = Command::new("cargo")
        .args(["run", "--", "--help"])
        .env("GITLAB_TOKEN", "sensitive-token-value")
        .output()
        .expect("failed to execute process");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Check that the token is NOT present in the output
    assert!(
        !stdout.contains("sensitive-token-value"),
        "GITLAB_TOKEN leaked in --help output!"
    );

    // The unsafe argv token option has been removed entirely.
    assert!(
        !stdout.contains("--token") && !stdout.contains("[env: GITLAB_TOKEN]"),
        "help must not advertise a token argument or echo credential state"
    );
}
