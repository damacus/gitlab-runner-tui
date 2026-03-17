mod client;
mod conductor;
mod config;
mod models;
mod tui;

use anyhow::{Context, Result};
use clap::Parser;
use client::GitLabClient;
use conductor::Conductor;
use config::AppConfig;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use models::runner::RunnerFilters;
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{env, io, time::Instant};
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

    // Priority: CLI flags > env vars > config.toml > defaults
    let host = args
        .host
        .or_else(|| env::var("GITLAB_HOST").ok())
        .or_else(|| config.gitlab_host.clone())
        .unwrap_or_else(|| "https://gitlab.com".to_string());

    let token = args
        .token
        .or_else(|| env::var("GITLAB_TOKEN").ok())
        .or_else(|| config.gitlab_token.clone())
        .context(
            "GITLAB_TOKEN must be set via environment variable, --token flag, or config.toml",
        )?;

    let client = GitLabClient::new(host, token)?;
    let conductor = Conductor::new(client);

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
}
