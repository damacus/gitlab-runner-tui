mod client;
mod conductor;
mod config;
mod models;
mod tui;

use anyhow::{Context, Result};
use clap::Parser;
use client::GitLabClient;
use conductor::Conductor;
use config::{AppConfig, RunnerTarget, RunnerTargetKind};
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use models::runner::RunnerFilters;
use ratatui::{backend::CrosstermBackend, Terminal};
use reqwest::StatusCode;
use std::{
    env,
    io::{self, Write},
    time::Instant,
};
use tui::{
    app::App,
    event::{Event, EventHandler},
    ui,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HeadlessCommand {
    Fetch,
    Switch,
    Flames,
    Empty,
    Rotate,
}

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(long, env("GITLAB_HOST"))]
    host: Option<String>,

    #[arg(long, env("GITLAB_TOKEN"))]
    token: Option<String>,

    /// Run in headless mode, polling until timeout
    #[arg(long)]
    watch: bool,

    /// Command to run in headless mode (fetch, lights, switch, workers, flames, empty, rotate)
    #[arg(long, default_value = "rotate")]
    command: String,

    /// Comma-separated tags to filter runners
    #[arg(long)]
    tags: Option<String>,
}

impl std::fmt::Debug for Args {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Args")
            .field("host", &self.host)
            .field("token", &self.token.as_ref().map(|_| "[REDACTED]"))
            .field("watch", &self.watch)
            .field("command", &self.command)
            .field("tags", &self.tags)
            .finish()
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let args = Args::parse();
    let config = AppConfig::load().unwrap_or_default();

    // Setup logging
    let file_appender = tracing_appender::rolling::daily("logs", "gitlab-runner-tui.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
    tracing_subscriber::fmt()
        .with_writer(non_blocking)
        .with_ansi(false)
        .init();

    let (host, token, config) = resolve_runtime_settings(&args, config)?;

    let conductor = if args.watch {
        let client = GitLabClient::new(host, token)?;
        Conductor::new(client, config.runner_targets.clone())
    } else {
        bootstrap_interactive_conductor(host, token, config.clone()).await?
    };

    if args.watch {
        return run_headless(conductor, config, &args.command, args.tags.as_deref()).await;
    }

    let mut app = App::new(conductor, config);
    app.execute_search().await;

    // Setup Terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut event_handler = EventHandler::new(std::time::Duration::from_millis(250));

    // Main Loop
    loop {
        terminal.draw(|frame| ui::render(&mut app, frame))?;

        if let Some(event) = event_handler.next().await {
            match event {
                Event::Key(key) => app.handle_key(key).await,
                Event::Tick => app.tick().await,
            }
        }

        if app.should_quit {
            break;
        }
    }

    // Stop event handler task
    event_handler.stop();

    // Restore Terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}

fn resolve_runtime_settings(args: &Args, config: AppConfig) -> Result<(String, String, AppConfig)> {
    resolve_runtime_settings_with_env(
        args,
        config,
        env::var("GITLAB_HOST").ok(),
        env::var("GITLAB_TOKEN").ok(),
    )
}

async fn bootstrap_interactive_conductor(
    mut host: String,
    mut token: String,
    mut config: AppConfig,
) -> Result<Conductor> {
    loop {
        let client = GitLabClient::new(host.clone(), token.clone())?;
        let conductor = Conductor::new(client, config.runner_targets.clone());

        match validate_interactive_credentials(&conductor).await {
            Ok(()) => return Ok(conductor),
            Err(error) if is_auth_error(&error) => {
                println!("GitLab authentication failed during startup.");
                println!(
                    "Your configured token may be missing, expired, or not accepted by the GitLab user API."
                );
                println!();

                let (updated_host, updated_token, updated_config) =
                    run_first_time_setup(host.clone(), &mut config)?;
                host = updated_host;
                token = updated_token;
                config = updated_config;
            }
            Err(error) => return Err(error),
        }
    }
}

async fn validate_interactive_credentials(conductor: &Conductor) -> Result<()> {
    conductor.validate_token().await?;
    Ok(())
}

fn is_auth_error(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause
            .downcast_ref::<reqwest::Error>()
            .and_then(|reqwest_error| reqwest_error.status())
            .is_some_and(|status| {
                status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN
            })
    })
}

fn resolve_runtime_settings_with_env(
    args: &Args,
    mut config: AppConfig,
    env_host: Option<String>,
    env_token: Option<String>,
) -> Result<(String, String, AppConfig)> {
    let host = args
        .host
        .clone()
        .or(env_host)
        .or_else(|| config.gitlab_host.clone())
        .unwrap_or_else(|| "https://gitlab.com".to_string());

    let token = args
        .token
        .clone()
        .or(env_token)
        .or_else(|| config.gitlab_token.clone());

    if args.watch {
        let token = token.context(
            "GITLAB_TOKEN must be set via environment variable, --token flag, or config.toml",
        )?;
        ensure_runner_targets_configured(&config)?;
        return Ok((host, token, config));
    }

    match (token, config.runner_targets.is_empty()) {
        (Some(token), false) => Ok((host, token, config)),
        (None, _) | (Some(_), true) => run_first_time_setup(host, &mut config),
    }
}

fn run_first_time_setup(
    host: String,
    config: &mut AppConfig,
) -> Result<(String, String, AppConfig)> {
    println!("GitLab configuration is incomplete. Starting setup.");
    println!("This will write a config file to the canonical gitlab-runner-tui config path.");
    println!();

    let host = prompt_with_default("GitLab host", &host)?;
    let token = prompt_required("GitLab personal access token (read_api scope)")?;
    let runner_targets = prompt_runner_targets()?;

    config.gitlab_host = Some(host.clone());
    config.gitlab_token = Some(token.clone());
    config.runner_targets = runner_targets;

    let saved_path = config.save_to_canonical_path()?;

    println!();
    println!("Saved configuration to {}", saved_path.display());
    println!("Launching the TUI...");
    println!();

    Ok((host, token, config.clone()))
}

fn ensure_runner_targets_configured(config: &AppConfig) -> Result<()> {
    if !config.has_runner_targets() {
        anyhow::bail!(
            "At least one runner target must be configured in config.toml. Use entries like group:my-org/platform or project:my-org/app during onboarding."
        );
    }

    Ok(())
}

fn prompt_with_default(prompt: &str, default: &str) -> Result<String> {
    print!("{} [{}]: ", prompt, default);
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    let trimmed = input.trim();
    if trimmed.is_empty() {
        Ok(default.to_string())
    } else {
        Ok(trimmed.to_string())
    }
}

fn prompt_required(prompt: &str) -> Result<String> {
    loop {
        print!("{}: ", prompt);
        io::stdout().flush()?;

        let input = rpassword::read_password()?;
        let trimmed = input.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }

        println!("A value is required.");
    }
}

fn prompt_runner_targets() -> Result<Vec<RunnerTarget>> {
    loop {
        print!("Runner targets (comma-separated, e.g. group:my-org/platform,project:my-org/app): ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        match parse_runner_targets(&input) {
            Ok(targets) if !targets.is_empty() => return Ok(targets),
            Ok(_) => println!("At least one runner target is required."),
            Err(error) => println!("{error}"),
        }
    }
}

fn parse_runner_targets(input: &str) -> Result<Vec<RunnerTarget>> {
    input
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(parse_runner_target)
        .collect()
}

fn parse_runner_target(entry: &str) -> Result<RunnerTarget> {
    let (kind, id) = entry
        .split_once(':')
        .context("Runner targets must use group:<id-or-path> or project:<id-or-path>")?;

    let id = id.trim();
    if id.is_empty() {
        anyhow::bail!("Runner target identifiers cannot be empty");
    }

    let kind = match kind.trim() {
        "group" => RunnerTargetKind::Group,
        "project" => RunnerTargetKind::Project,
        other => anyhow::bail!("Unsupported runner target kind: {other}"),
    };

    Ok(RunnerTarget {
        kind,
        id: id.to_string(),
        label: None,
    })
}

async fn run_headless(
    conductor: Conductor,
    config: AppConfig,
    command: &str,
    tags: Option<&str>,
) -> Result<()> {
    let command = parse_headless_command(command)?;
    let poll_interval = std::time::Duration::from_secs(config.poll_interval_secs);
    let started_at = Instant::now();
    let mut iteration = 0u64;

    loop {
        iteration += 1;
        let elapsed = started_at.elapsed().as_secs();

        let filters = build_runner_filters(tags);

        let result = match command {
            HeadlessCommand::Fetch => conductor.fetch_runners(filters).await,
            HeadlessCommand::Switch => conductor.list_offline_runners(filters).await,
            HeadlessCommand::Flames => conductor.list_uncontacted_runners(filters, 3600).await,
            HeadlessCommand::Empty => conductor.list_runners_without_managers(filters).await,
            HeadlessCommand::Rotate => conductor.detect_rotating_runners(filters).await,
        };

        match result {
            Ok(runners) => {
                println!(
                    "[{:02}:{:02}] Poll #{} — {} runners matched (command: {})",
                    elapsed / 60,
                    elapsed % 60,
                    iteration,
                    runners.len(),
                    command.as_str(),
                );

                for runner in &runners {
                    let mgr_info: Vec<String> = runner
                        .managers
                        .iter()
                        .map(|m| {
                            format!(
                                "{}({}/{})",
                                m.system_id,
                                m.status,
                                m.version.as_deref().unwrap_or("-")
                            )
                        })
                        .collect();

                    println!(
                        "  Runner {} [{}] managers=[{}]",
                        runner.id,
                        runner.tag_list.join(","),
                        mgr_info.join(", ")
                    );
                }

                if runners.is_empty() && command == HeadlessCommand::Rotate {
                    println!("  ✓ No rotation detected — all runners have single managers");
                }
            }
            Err(e) => {
                eprintln!("Error: {:#}", e);
            }
        }

        // Check timeout
        if started_at.elapsed().as_secs() >= config.poll_timeout_secs {
            println!(
                "\nPoll timeout reached ({} seconds). Exiting.",
                config.poll_timeout_secs
            );
            break;
        }

        tokio::time::sleep(poll_interval).await;
    }

    Ok(())
}

impl HeadlessCommand {
    fn as_str(self) -> &'static str {
        match self {
            HeadlessCommand::Fetch => "fetch",
            HeadlessCommand::Switch => "switch",
            HeadlessCommand::Flames => "flames",
            HeadlessCommand::Empty => "empty",
            HeadlessCommand::Rotate => "rotate",
        }
    }
}

fn parse_headless_command(command: &str) -> Result<HeadlessCommand> {
    match command {
        "fetch" => Ok(HeadlessCommand::Fetch),
        "switch" => Ok(HeadlessCommand::Switch),
        "flames" => Ok(HeadlessCommand::Flames),
        "empty" => Ok(HeadlessCommand::Empty),
        "rotate" => Ok(HeadlessCommand::Rotate),
        other => anyhow::bail!(
            "Unknown headless command: {}. Supported commands: fetch, switch, flames, empty, rotate",
            other
        ),
    }
}

fn build_runner_filters(tags: Option<&str>) -> RunnerFilters {
    let tag_list = tags.and_then(|tag_str| {
        let tags: Vec<String> = tag_str
            .split(',')
            .map(str::trim)
            .filter(|tag| !tag.is_empty())
            .map(ToOwned::to_owned)
            .collect();

        if tags.is_empty() {
            None
        } else {
            Some(tags)
        }
    });

    RunnerFilters {
        tag_list,
        ..RunnerFilters::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::Server;

    #[test]
    fn parses_supported_headless_commands() {
        assert_eq!(
            parse_headless_command("fetch").unwrap(),
            HeadlessCommand::Fetch
        );
        assert_eq!(
            parse_headless_command("switch").unwrap(),
            HeadlessCommand::Switch
        );
        assert_eq!(
            parse_headless_command("flames").unwrap(),
            HeadlessCommand::Flames
        );
        assert_eq!(
            parse_headless_command("empty").unwrap(),
            HeadlessCommand::Empty
        );
        assert_eq!(
            parse_headless_command("rotate").unwrap(),
            HeadlessCommand::Rotate
        );
    }

    #[test]
    fn rejects_non_headless_commands() {
        let error = parse_headless_command("lights").unwrap_err().to_string();
        assert!(error.contains("Unknown headless command: lights"));
        assert!(error.contains("fetch, switch, flames, empty, rotate"));
    }

    #[test]
    fn rejects_headless_commands_with_case_or_whitespace_mismatch() {
        assert!(parse_headless_command("Fetch").is_err());
        assert!(parse_headless_command("fetch ").is_err());
        assert!(parse_headless_command(" rotate").is_err());
    }

    #[test]
    fn builds_empty_filters_when_tags_missing_or_blank() {
        assert_eq!(build_runner_filters(None), RunnerFilters::default());
        assert_eq!(
            build_runner_filters(Some(" , , ")),
            RunnerFilters::default()
        );
    }

    #[test]
    fn builds_trimmed_tag_filters() {
        let filters = build_runner_filters(Some(" alm, production ,, linux "));
        assert_eq!(
            filters.tag_list,
            Some(vec![
                "alm".to_string(),
                "production".to_string(),
                "linux".to_string()
            ])
        );
        assert_eq!(filters.status, None);
        assert_eq!(filters.runner_type, None);
        assert_eq!(filters.version_prefix, None);
        assert_eq!(filters.paused, None);
    }

    #[test]
    fn builds_tag_filters_preserving_order_and_duplicates() {
        let filters = build_runner_filters(Some("prod, staging,prod,qa"));
        assert_eq!(
            filters.tag_list,
            Some(vec![
                "prod".to_string(),
                "staging".to_string(),
                "prod".to_string(),
                "qa".to_string()
            ])
        );
    }

    #[test]
    fn builds_tag_filters_with_tab_and_newline_whitespace() {
        let filters = build_runner_filters(Some("\tprod,\nqa,  staging "));
        assert_eq!(
            filters.tag_list,
            Some(vec![
                "prod".to_string(),
                "qa".to_string(),
                "staging".to_string()
            ])
        );
    }

    #[test]
    fn parses_mixed_runner_targets() {
        let targets = parse_runner_targets("group:my-org/platform, project:42").unwrap();

        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].kind, RunnerTargetKind::Group);
        assert_eq!(targets[0].id, "my-org/platform");
        assert_eq!(targets[1].kind, RunnerTargetKind::Project);
        assert_eq!(targets[1].id, "42");
    }

    #[test]
    fn rejects_runner_targets_without_kind_prefix() {
        let error = parse_runner_targets("my-org/platform")
            .unwrap_err()
            .to_string();
        assert!(error.contains("group:<id-or-path> or project:<id-or-path>"));
    }

    #[test]
    fn resolves_runtime_settings_from_config_for_interactive_mode() {
        let args = Args {
            host: None,
            token: None,
            watch: false,
            command: "rotate".to_string(),
            tags: None,
        };
        let config = AppConfig {
            gitlab_host: Some("https://gitlab.example.com".to_string()),
            gitlab_token: Some("glpat-config-token".to_string()),
            runner_targets: vec![RunnerTarget {
                kind: RunnerTargetKind::Group,
                id: "my-org/platform".to_string(),
                label: None,
            }],
            ..AppConfig::default()
        };

        let (host, token, resolved_config) =
            resolve_runtime_settings_with_env(&args, config.clone(), None, None).unwrap();

        assert_eq!(host, "https://gitlab.example.com");
        assert_eq!(token, "glpat-config-token");
        assert_eq!(resolved_config, config);
    }

    #[test]
    fn watch_mode_requires_token_when_not_configured() {
        let args = Args {
            host: None,
            token: None,
            watch: true,
            command: "rotate".to_string(),
            tags: None,
        };

        let error = resolve_runtime_settings_with_env(&args, AppConfig::default(), None, None)
            .unwrap_err()
            .to_string();

        assert!(error.contains("GITLAB_TOKEN must be set"));
    }

    #[test]
    fn watch_mode_requires_runner_targets() {
        let args = Args {
            host: None,
            token: Some("glpat-test".to_string()),
            watch: true,
            command: "rotate".to_string(),
            tags: None,
        };

        let error = resolve_runtime_settings_with_env(&args, AppConfig::default(), None, None)
            .unwrap_err()
            .to_string();

        assert!(error.contains("At least one runner target must be configured"));
    }

    #[tokio::test]
    async fn detects_unauthorized_reqwest_errors() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("GET", "/api/v4/user")
            .match_header("PRIVATE-TOKEN", "bad-token")
            .with_status(401)
            .with_body(r#"{"message":"401 Unauthorized"}"#)
            .create_async()
            .await;

        let client = GitLabClient::new(server.url(), "bad-token".to_string()).unwrap();
        let conductor = Conductor::new(
            client,
            vec![RunnerTarget {
                kind: RunnerTargetKind::Group,
                id: "123".to_string(),
                label: None,
            }],
        );
        let error = validate_interactive_credentials(&conductor)
            .await
            .unwrap_err();

        mock.assert_async().await;
        assert!(is_auth_error(&error));
    }

    #[test]
    fn does_not_treat_generic_errors_as_auth_errors() {
        let error = anyhow::anyhow!("network exploded");
        assert!(!is_auth_error(&error));
    }
}
