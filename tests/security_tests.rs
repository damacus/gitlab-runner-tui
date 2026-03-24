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

    // Verify that the environment variable is acknowledged as set but value is hidden
    assert!(
        stdout.contains("[env: GITLAB_TOKEN]"),
        "GITLAB_TOKEN should be shown as an env var but hidden"
    );
}
