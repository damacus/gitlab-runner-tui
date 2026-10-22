use std::path::Path;
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

fn auth_status(
    current_dir: &Path,
    config_path: &Path,
    dotenv_path: Option<&Path>,
    inherited_host: Option<&str>,
) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_gitlab-runner-tui"));
    command
        .current_dir(current_dir)
        .arg("--config")
        .arg(config_path)
        .env("GITLAB_TOKEN", "test-process-token")
        .env_remove("GITLAB_HOST");

    if let Some(dotenv_path) = dotenv_path {
        command.arg("--dotenv").arg(dotenv_path);
    }
    if let Some(inherited_host) = inherited_host {
        command.env("GITLAB_HOST", inherited_host);
    }

    command
        .args(["auth", "status"])
        .output()
        .expect("failed to execute gitlab-runner-tui")
}

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

#[test]
fn dotenv_is_explicit_and_preserves_existing_environment_values() {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!("gitlab-runner-tui-dotenv-{suffix}"));
    let dotenv_path = directory.join(".env");
    let config_path = directory.join("config.toml");
    let dotenv_host = format!("https://dotenv-{suffix}.example.test");
    let inherited_host = format!("https://process-{suffix}.example.test");

    std::fs::create_dir(&directory).unwrap();
    std::fs::write(&dotenv_path, format!("GITLAB_HOST={dotenv_host}\n")).unwrap();

    let unselected = auth_status(&directory, &config_path, None, None);
    assert!(unselected.status.success());
    assert!(String::from_utf8_lossy(&unselected.stdout).contains("GitLab host: https://gitlab.com"));

    let selected = auth_status(&directory, &config_path, Some(&dotenv_path), None);
    assert!(selected.status.success());
    assert!(
        String::from_utf8_lossy(&selected.stdout).contains(&format!("GitLab host: {dotenv_host}"))
    );

    let inherited = auth_status(
        &directory,
        &config_path,
        Some(&dotenv_path),
        Some(&inherited_host),
    );
    assert!(inherited.status.success());
    assert!(String::from_utf8_lossy(&inherited.stdout)
        .contains(&format!("GitLab host: {inherited_host}")));

    std::fs::remove_dir_all(directory).unwrap();
}
