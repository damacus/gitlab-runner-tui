mod client;
mod conductor;
mod config;
mod fixtures;
mod metrics;
mod models;
mod rotation_wait;
mod tui;

#[cfg(test)]
mod allocation_counter {
    use std::{
        alloc::{GlobalAlloc, Layout, System},
        cell::Cell,
    };

    thread_local! {
        static ENABLED: Cell<bool> = const { Cell::new(false) };
        static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
    }

    pub struct CountingAllocator;

    unsafe impl GlobalAlloc for CountingAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            ENABLED.with(|enabled| {
                if enabled.get() {
                    ALLOCATIONS.with(|count| count.set(count.get() + 1));
                }
            });
            unsafe { System.alloc(layout) }
        }

        unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
            unsafe { System.dealloc(pointer, layout) }
        }
    }

    pub fn count_allocations<T>(operation: impl FnOnce() -> T) -> (T, usize) {
        ALLOCATIONS.with(|count| count.set(0));
        ENABLED.with(|enabled| enabled.set(true));
        let result = operation();
        ENABLED.with(|enabled| enabled.set(false));
        let allocations = ALLOCATIONS.with(Cell::get);
        (result, allocations)
    }
}

#[cfg(test)]
#[global_allocator]
static TEST_ALLOCATOR: allocation_counter::CountingAllocator =
    allocation_counter::CountingAllocator;

const DEMO_HOST: &str = "https://demo.gitlab.example.com";

use anyhow::{Context, Result};
use chrono::{DateTime, Local, LocalResult, NaiveTime, TimeZone, Utc};
use clap::{Parser, Subcommand};
use client::GitLabClient;
use conductor::{Conductor, QueryOutcome, QueryProfile};
use config::{
    parse_runner_targets, AppConfig, RotationWaitConfig, RunnerDiscoveryMode, RunnerTarget,
};
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use models::runner::{parse_stale_cutoff, ContactThreshold, RunnerFilters};
use ratatui::{backend::CrosstermBackend, Terminal};
use reqwest::StatusCode;
use rotation_wait::{RotationWaitOptions, RotationWaitState};
use std::{
    env,
    io::{self, Write},
    path::PathBuf,
};
use tui::{
    app::App,
    event::{Event, EventHandler},
    ui,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Subcommand)]
enum CliCommand {
    /// List all discovered runners
    Fetch {
        /// Return list endpoint data only without per-runner detail or manager requests
        #[arg(long)]
        summary: bool,
    },
    /// List offline runners where all managers are offline
    Switch,
    /// List runners whose managers have not contacted GitLab recently
    Flames,
    /// List runners with no registered managers
    Empty,
    /// List runners with more than one manager
    Rotating {
        /// Poll until all matching runners have rotated
        #[arg(long)]
        wait: bool,
    },
}

#[derive(Parser)]
#[command(version, about, long_about = None, disable_version_flag = true)]
struct Args {
    #[command(subcommand)]
    command: Option<CliCommand>,

    #[arg(long, env("GITLAB_HOST"), global = true)]
    host: Option<String>,

    /// Load configuration from this explicitly trusted file instead of the canonical user config
    #[arg(long, value_name = "PATH", global = true)]
    config: Option<PathBuf>,

    /// Load environment variables from this explicitly trusted dotenv file
    #[arg(long, value_name = "PATH", global = true)]
    dotenv: Option<PathBuf>,

    /// Comma-separated tags to filter runners
    #[arg(long, global = true)]
    tags: Option<String>,

    /// Version prefix to filter runners (e.g. "16.11")
    #[arg(long = "version", global = true)]
    version_filter: Option<String>,

    /// Cutoff for flames: HH:MM, HH:MM:SS, or RFC3339 timestamp
    #[arg(long, global = true)]
    stale_cutoff: Option<String>,

    /// Run with demo fixture data (no GitLab credentials required)
    #[arg(long, global = true)]
    demo: bool,
}

impl std::fmt::Debug for Args {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Args")
            .field("host", &self.host)
            .field("config", &self.config)
            .field("dotenv", &self.dotenv)
            .field("command", &self.command)
            .field("tags", &self.tags)
            .field("version", &self.version_filter)
            .field("stale_cutoff", &self.stale_cutoff)
            .field("demo", &self.demo)
            .finish()
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    load_explicit_dotenv(args.dotenv.as_deref())?;
    let mut config = match args.config.as_deref() {
        Some(path) => AppConfig::load_from_path(path)?,
        None => AppConfig::load()?,
    };
    validate_cli_args(&args)?;
    config.validate_enrichment_request_limit()?;

    // Setup logging
    let file_appender = tracing_appender::rolling::daily("logs", "gitlab-runner-tui.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
    tracing_subscriber::fmt()
        .with_writer(non_blocking)
        .with_ansi(false)
        .init();

    let is_command_mode = args.command.is_some();
    if is_command_mode {
        config.validate_runtime_settings()?;
    }

    let conductor = if args.demo {
        let client = GitLabClient::new(DEMO_HOST.to_string(), "demo-token".to_string())?;
        let mut c = Conductor::new_with_mode_and_enrichment_limit(
            client,
            config.discovery_mode,
            config.runner_targets.clone(),
            config.max_enrichment_requests,
        );
        c.demo_mode = true;
        c
    } else {
        let (host, token, resolved_config) = resolve_runtime_settings(&args, config.clone())?;
        config = resolved_config;
        if is_command_mode {
            let client = GitLabClient::new(host, token)?;
            Conductor::new_with_mode_and_enrichment_limit(
                client,
                config.discovery_mode,
                config.runner_targets.clone(),
                config.max_enrichment_requests,
            )
        } else {
            bootstrap_interactive_conductor(host, token, config.clone()).await?
        }
    };

    if let Some(command) = args.command {
        if matches!(command, CliCommand::Rotating { wait: true }) {
            let completed = run_rotation_wait(
                &conductor,
                &config,
                args.tags.as_deref(),
                args.version_filter.as_deref(),
            )
            .await?;
            if !completed {
                std::process::exit(2);
            }
        } else {
            let outcome = run_cli_query(
                &conductor,
                command,
                args.tags.as_deref(),
                args.version_filter.as_deref(),
                args.stale_cutoff.as_deref(),
            )
            .await?;
            println!("{}", render_json_output(&outcome)?);
        }
        return Ok(());
    }

    let mut app = App::new(conductor, config);
    if args.demo {
        app.demo_mode = true;
        app.seed_demo_data(fixtures::demo_runners());
    } else {
        app.start_search();
    }
    run_tui(app).await
}

async fn run_tui(mut app: App) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let mut event_handler = EventHandler::new(std::time::Duration::from_millis(250));

    loop {
        if app.take_redraw_request() {
            terminal.draw(|frame| ui::render(&mut app, frame))?;
        }

        if let Some(event) = event_handler.next().await {
            match event {
                Event::Key(key) => app.handle_key(key).await,
                Event::Resize(width, height) => app.handle_resize(width, height),
                Event::Tick => app.tick().await,
            }
        }

        if app.should_quit {
            break;
        }
    }

    event_handler.stop().await;
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
        let conductor = Conductor::new_with_mode_and_enrichment_limit(
            client,
            config.discovery_mode,
            config.runner_targets.clone(),
            config.max_enrichment_requests,
        );

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

    let token = env_token.or_else(|| config.gitlab_token.clone());

    if args.command.is_some() {
        config.validate_runtime_settings()?;
        let token = token
            .context("GITLAB_TOKEN must be set via environment variable or trusted config.toml")?;
        return Ok((host, token, config));
    }

    match token {
        Some(token) => Ok((host, token, config)),
        None => run_first_time_setup(host, &mut config),
    }
}

fn load_explicit_dotenv(path: Option<&std::path::Path>) -> Result<()> {
    if let Some(path) = path {
        dotenvy::from_path(path)
            .with_context(|| format!("Failed to load dotenv file {}", path.display()))?;
    }
    Ok(())
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
    let discovery_mode = prompt_discovery_mode(config.discovery_mode)?;
    let runner_targets =
        prompt_runner_targets(discovery_mode == RunnerDiscoveryMode::ConfiguredTargets)?;

    config.gitlab_host = Some(host.clone());
    config.gitlab_token = Some(token.clone());
    config.discovery_mode = discovery_mode;
    config.runner_targets = runner_targets;
    config.validate_runtime_settings()?;

    let saved_path = config.save_to_canonical_path()?;

    println!();
    println!("Saved configuration to {}", saved_path.display());
    println!("Launching the TUI...");
    println!();

    Ok((host, token, config.clone()))
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

fn prompt_discovery_mode(default: RunnerDiscoveryMode) -> Result<RunnerDiscoveryMode> {
    loop {
        let default_label = match default {
            RunnerDiscoveryMode::AllRunners => "all",
            RunnerDiscoveryMode::ConfiguredTargets => "targets",
            RunnerDiscoveryMode::VisibleRunners => "visible",
        };

        print!("Discovery mode [all/targets/visible] [{}]: ", default_label);
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        match input.trim() {
            "" => return Ok(default),
            "all" | "/runners/all" => return Ok(RunnerDiscoveryMode::AllRunners),
            "targets" | "target" | "configured" => {
                return Ok(RunnerDiscoveryMode::ConfiguredTargets)
            }
            "visible" | "runners" | "/runners" => return Ok(RunnerDiscoveryMode::VisibleRunners),
            _ => println!("Choose 'all', 'targets', or 'visible'."),
        }
    }
}

fn prompt_runner_targets(required: bool) -> Result<Vec<RunnerTarget>> {
    loop {
        let prompt = if required {
            "Runner targets (comma-separated, e.g. group:my-org/platform,project:my-org/app): "
        } else {
            "Runner targets (optional, comma-separated, e.g. group:my-org/platform,project:my-org/app): "
        };
        print!("{prompt}");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        if !required && input.trim().is_empty() {
            return Ok(Vec::new());
        }

        match parse_runner_targets(&input) {
            Ok(targets) if required && targets.is_empty() => {
                println!("At least one target is required in targets mode.");
            }
            Ok(targets) => return Ok(targets),
            Err(error) => println!("{error}"),
        }
    }
}

fn validate_cli_args(args: &Args) -> Result<()> {
    if matches!(args.command, Some(CliCommand::Rotating { .. }))
        && build_runner_filters(args.tags.as_deref(), None)
            .tag_list
            .is_none()
    {
        anyhow::bail!("rotating requires --tags with at least one non-empty tag");
    }

    if matches!(args.command, Some(CliCommand::Fetch { summary: true }))
        && args.version_filter.is_some()
    {
        anyhow::bail!("fetch --summary cannot be combined with --version");
    }

    if args.stale_cutoff.is_some() && args.command != Some(CliCommand::Flames) {
        anyhow::bail!("--stale-cutoff is only supported with flames");
    }

    Ok(())
}

async fn run_cli_query(
    conductor: &Conductor,
    command: CliCommand,
    tags: Option<&str>,
    version: Option<&str>,
    stale_cutoff: Option<&str>,
) -> Result<QueryOutcome> {
    let filters = build_runner_filters(tags, version);

    if matches!(command, CliCommand::Rotating { .. }) && filters.tag_list.is_none() {
        anyhow::bail!("rotating requires --tags with at least one non-empty tag");
    }
    if matches!(command, CliCommand::Fetch { summary: true }) && filters.version_prefix.is_some() {
        anyhow::bail!("fetch --summary cannot be combined with --version");
    }

    let stale_threshold = build_stale_threshold(command, stale_cutoff)?;

    match command {
        CliCommand::Fetch { summary: true } => {
            conductor.fetch_runner_summaries_with_metrics(filters).await
        }
        CliCommand::Fetch { summary: false } => conductor.fetch_runners_with_metrics(filters).await,
        CliCommand::Switch => conductor.list_offline_runners_with_metrics(filters).await,
        CliCommand::Flames => {
            conductor
                .list_uncontacted_runners_with_metrics(filters, stale_threshold)
                .await
        }
        CliCommand::Empty => {
            conductor
                .list_runners_without_managers_with_metrics(filters)
                .await
        }
        CliCommand::Rotating { .. } => {
            conductor
                .detect_rotating_runners_with_metrics(filters)
                .await
        }
    }
}

async fn run_rotation_wait(
    conductor: &Conductor,
    config: &AppConfig,
    tags: Option<&str>,
    version: Option<&str>,
) -> Result<bool> {
    let started_at = std::time::Instant::now();
    let filters = build_runner_filters(tags, version);
    let options = build_rotation_wait_options(&config.rotation_wait, Utc::now())?;
    let mut state = RotationWaitState::new(options);
    let poll_interval = std::time::Duration::from_secs(config.poll_interval_secs);

    loop {
        let outcome = conductor
            .fetch_runners_with_profile_and_metrics(filters.clone(), QueryProfile::Managers)
            .await?;
        let event = state.observe(&outcome.runners, Utc::now());
        println!("{}", serde_json::to_string(&event)?);

        if event.is_complete {
            return Ok(true);
        }

        if started_at.elapsed().as_secs() >= config.poll_timeout_secs {
            println!("{}", serde_json::to_string(&state.timeout_event())?);
            return Ok(false);
        }

        tokio::time::sleep(poll_interval).await;
    }
}

fn build_rotation_wait_options(
    config: &RotationWaitConfig,
    command_started_at: DateTime<Utc>,
) -> Result<RotationWaitOptions> {
    Ok(RotationWaitOptions {
        rotation_window_start: resolve_rotation_window_start(config, command_started_at)?,
        active_contacted_within_secs: config.active_contacted_within_secs,
        missing_runner_grace_polls: config.missing_runner_grace_polls,
        completion_stability_polls: config.completion_stability_polls,
    })
}

fn resolve_rotation_window_start(
    config: &RotationWaitConfig,
    command_started_at: DateTime<Utc>,
) -> Result<DateTime<Utc>> {
    let Some(start) = config.rotation_window_start.as_deref() else {
        return Ok(command_started_at);
    };

    let time = NaiveTime::parse_from_str(start.trim(), "%H:%M:%S")
        .or_else(|_| NaiveTime::parse_from_str(start.trim(), "%H:%M"))
        .with_context(|| "rotation_window_start must use HH:MM or HH:MM:SS")?;

    match config
        .timezone
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(timezone) => {
            let tz: chrono_tz::Tz = timezone
                .parse()
                .with_context(|| format!("Unsupported rotation_wait timezone: {timezone}"))?;
            let local_now = command_started_at.with_timezone(&tz);
            let local_window = local_now.date_naive().and_time(time);
            match tz.from_local_datetime(&local_window) {
                LocalResult::Single(value) => Ok(value.with_timezone(&Utc)),
                LocalResult::Ambiguous(earliest, _) => Ok(earliest.with_timezone(&Utc)),
                LocalResult::None => anyhow::bail!(
                    "rotation_window_start does not exist in timezone {timezone} today"
                ),
            }
        }
        None => {
            let local_now = command_started_at.with_timezone(&Local);
            let local_window = local_now.date_naive().and_time(time);
            match Local.from_local_datetime(&local_window) {
                LocalResult::Single(value) => Ok(value.with_timezone(&Utc)),
                LocalResult::Ambiguous(earliest, _) => Ok(earliest.with_timezone(&Utc)),
                LocalResult::None => anyhow::bail!(
                    "rotation_window_start does not exist in the system timezone today"
                ),
            }
        }
    }
}

fn render_json_output(outcome: &QueryOutcome) -> Result<String> {
    Ok(serde_json::to_string_pretty(outcome)?)
}

fn build_stale_threshold(
    command: CliCommand,
    stale_cutoff: Option<&str>,
) -> Result<ContactThreshold> {
    match (command, stale_cutoff) {
        (CliCommand::Flames, Some(input)) => {
            let cutoff = parse_stale_cutoff(input, Local::now())
                .map_err(anyhow::Error::msg)?
                .context("--stale-cutoff cannot be blank")?;
            Ok(ContactThreshold::Since(cutoff))
        }
        (CliCommand::Flames, None) => Ok(ContactThreshold::OlderThanSecs(3600)),
        (_, Some(_)) => anyhow::bail!("--stale-cutoff is only supported with flames"),
        (_, None) => Ok(ContactThreshold::OlderThanSecs(3600)),
    }
}

fn build_runner_filters(tags: Option<&str>, version: Option<&str>) -> RunnerFilters {
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
        version_prefix: version.map(|v| v.to_string()),
        ..RunnerFilters::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::RunnerTargetKind;
    use mockito::{Matcher, Server};

    fn list_response_body(id: u64, status: &str) -> String {
        format!(
            r#"{{
                "id": {},
                "runner_type": "group_type",
                "active": true,
                "paused": false,
                "description": "Runner {}",
                "ip_address": "",
                "is_shared": false,
                "status": "{}",
                "name": null,
                "online": {}
            }}"#,
            id,
            id,
            status,
            status == "online"
        )
    }

    fn detail_response_body(id: u64, status: &str, tags: &[&str]) -> String {
        let tags_json: Vec<String> = tags.iter().map(|tag| format!("\"{}\"", tag)).collect();
        format!(
            r#"{{
                "id": {},
                "runner_type": "group_type",
                "active": true,
                "paused": false,
                "description": "Runner {}",
                "ip_address": "",
                "is_shared": false,
                "status": "{}",
                "version": "17.5.0",
                "revision": "abc123",
                "tag_list": [{}]
            }}"#,
            id,
            id,
            status,
            tags_json.join(", ")
        )
    }

    fn manager_response_body(id: u64, runner_id: u64, status: &str) -> String {
        manager_response_body_with_times(
            id,
            runner_id,
            status,
            "2024-01-15T10:30:00.000Z",
            "2024-01-20T14:22:00.000Z",
        )
    }

    fn manager_response_body_with_times(
        id: u64,
        runner_id: u64,
        status: &str,
        created_at: &str,
        contacted_at: &str,
    ) -> String {
        format!(
            r#"{{
                "id": {},
                "system_id": "host-{}",
                "created_at": "{}",
                "contacted_at": "{}",
                "ip_address": "10.0.1.1",
                "status": "{}",
                "version": "17.5.0",
                "revision": "abc123"
            }}"#,
            id, runner_id, created_at, contacted_at, status
        )
    }

    async fn setup_command_runner_mocks(
        server: &mut Server,
        tags: Option<&str>,
        expected_detail_ids: &[u64],
    ) -> Vec<mockito::Mock> {
        let list_body = format!(
            "[{},{}]",
            list_response_body(1, "online"),
            list_response_body(2, "online")
        );
        let mut query_matchers = vec![
            Matcher::UrlEncoded("per_page".into(), "100".into()),
            Matcher::UrlEncoded("page".into(), "1".into()),
        ];
        if let Some(tag) = tags {
            query_matchers.push(Matcher::UrlEncoded("tag_list[]".into(), tag.into()));
        }

        let mut mocks = vec![
            server
                .mock("GET", "/api/v4/groups/123/runners")
                .match_query(Matcher::AllOf(query_matchers))
                .with_status(200)
                .with_body(list_body)
                .create_async()
                .await,
        ];

        for (id, manager_ids) in [(1, vec![11, 12]), (2, vec![21])] {
            mocks.push(
                server
                    .mock("GET", format!("/api/v4/runners/{}", id).as_str())
                    .with_status(200)
                    .with_body(detail_response_body(id, "online", &["prod"]))
                    .expect(usize::from(expected_detail_ids.contains(&id)))
                    .create_async()
                    .await,
            );

            let manager_body = manager_ids
                .into_iter()
                .map(|manager_id| manager_response_body(manager_id, id, "online"))
                .collect::<Vec<_>>()
                .join(",");
            mocks.push(
                server
                    .mock("GET", format!("/api/v4/runners/{}/managers", id).as_str())
                    .with_status(200)
                    .with_body(format!("[{}]", manager_body))
                    .create_async()
                    .await,
            );
        }

        mocks
    }

    fn command_test_conductor(server_url: String) -> Conductor {
        let client = GitLabClient::new(server_url, "test-token".to_string()).unwrap();
        Conductor::new_with_mode(
            client,
            RunnerDiscoveryMode::ConfiguredTargets,
            vec![RunnerTarget {
                kind: RunnerTargetKind::Group,
                id: "123".to_string(),
                label: None,
            }],
        )
    }

    #[test]
    fn parses_no_subcommand_as_tui_mode() {
        let args = Args::try_parse_from(["gitlab-runner-tui"]).unwrap();

        assert_eq!(args.command, None);
    }

    #[test]
    fn parses_supported_cli_commands() {
        for (name, expected) in [
            ("fetch", CliCommand::Fetch { summary: false }),
            ("switch", CliCommand::Switch),
            ("flames", CliCommand::Flames),
            ("empty", CliCommand::Empty),
            ("rotating", CliCommand::Rotating { wait: false }),
        ] {
            let args = Args::try_parse_from(["gitlab-runner-tui", name]).unwrap();
            assert_eq!(args.command, Some(expected));
        }
    }

    #[test]
    fn parses_fetch_summary_option() {
        let args = Args::try_parse_from(["gitlab-runner-tui", "fetch", "--summary"]).unwrap();

        assert_eq!(args.command, Some(CliCommand::Fetch { summary: true }));
    }

    #[test]
    fn parses_rotating_wait_option() {
        let args =
            Args::try_parse_from(["gitlab-runner-tui", "rotating", "--wait", "--tags", "prod"])
                .unwrap();

        assert_eq!(args.command, Some(CliCommand::Rotating { wait: true }));
        assert_eq!(args.tags, Some("prod".to_string()));
    }

    #[test]
    fn rejects_removed_once_json_and_command_flags() {
        assert!(Args::try_parse_from(["gitlab-runner-tui", "--once", "fetch"]).is_err());
        assert!(Args::try_parse_from(["gitlab-runner-tui", "--json", "fetch"]).is_err());
        assert!(Args::try_parse_from(["gitlab-runner-tui", "--command", "fetch"]).is_err());
        assert!(Args::try_parse_from(["gitlab-runner-tui", "--token", "sentinel-secret"]).is_err());
    }

    #[test]
    fn parses_only_explicit_local_config_and_dotenv_paths() {
        let args = Args::try_parse_from([
            "gitlab-runner-tui",
            "--config",
            "/trusted/config.toml",
            "--dotenv",
            "/trusted/runtime.env",
        ])
        .unwrap();

        assert_eq!(args.config, Some(PathBuf::from("/trusted/config.toml")));
        assert_eq!(args.dotenv, Some(PathBuf::from("/trusted/runtime.env")));
    }

    #[test]
    fn dotenv_file_is_ignored_unless_explicitly_selected() {
        const VARIABLE: &str = "GITLAB_RUNNER_TUI_EXPLICIT_DOTENV_TEST";
        let path = std::env::temp_dir().join(format!(
            "gitlab-runner-tui-explicit-dotenv-{}.env",
            std::process::id()
        ));
        std::fs::write(&path, format!("{VARIABLE}=loaded-explicitly\n")).unwrap();
        std::env::remove_var(VARIABLE);

        load_explicit_dotenv(None).unwrap();
        assert!(std::env::var(VARIABLE).is_err());

        load_explicit_dotenv(Some(&path)).unwrap();
        assert_eq!(std::env::var(VARIABLE).unwrap(), "loaded-explicitly");

        std::env::remove_var(VARIABLE);
        std::fs::remove_file(path).unwrap();
    }

    #[tokio::test]
    async fn rotating_requires_non_empty_tags_before_querying() {
        let conductor = command_test_conductor("http://127.0.0.1:1".to_string());

        let missing_error = match run_cli_query(
            &conductor,
            CliCommand::Rotating { wait: false },
            None,
            None,
            None,
        )
        .await
        {
            Ok(_) => panic!("rotating without tags should fail"),
            Err(error) => error.to_string(),
        };
        assert!(missing_error.contains("rotating requires --tags"));

        let blank_error = match run_cli_query(
            &conductor,
            CliCommand::Rotating { wait: false },
            Some(" , "),
            None,
            None,
        )
        .await
        {
            Ok(_) => panic!("rotating with blank tags should fail"),
            Err(error) => error.to_string(),
        };
        assert!(blank_error.contains("rotating requires --tags"));
    }

    #[tokio::test]
    async fn fetch_command_returns_json_envelope_for_all_runners() {
        let mut server = Server::new_async().await;
        let mocks = setup_command_runner_mocks(&mut server, None, &[1, 2]).await;
        let conductor = command_test_conductor(server.url());

        let outcome = run_cli_query(
            &conductor,
            CliCommand::Fetch { summary: false },
            None,
            None,
            None,
        )
        .await
        .unwrap();
        let output = render_json_output(&outcome).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

        assert_eq!(outcome.runners.len(), 2);
        assert_eq!(outcome.metrics.result_count, 2);
        assert_eq!(outcome.metrics.request_counts.detail_requests, 2);
        assert_eq!(outcome.metrics.request_counts.manager_requests, 2);
        assert!(parsed.get("runners").is_some());
        assert!(parsed.get("metrics").is_some());
        assert!(parsed.get("all_runners_fell_back").is_some());

        for mock in mocks {
            mock.assert_async().await;
        }
    }

    #[tokio::test]
    async fn fetch_summary_command_returns_list_only_json_envelope() {
        let mut server = Server::new_async().await;
        let list = server
            .mock("GET", "/api/v4/groups/123/runners")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("per_page".into(), "100".into()),
                Matcher::UrlEncoded("page".into(), "1".into()),
            ]))
            .with_status(200)
            .with_body(format!(
                "[{},{}]",
                list_response_body(1, "online"),
                list_response_body(2, "offline")
            ))
            .create_async()
            .await;
        let conductor = command_test_conductor(server.url());

        let outcome = run_cli_query(
            &conductor,
            CliCommand::Fetch { summary: true },
            None,
            None,
            None,
        )
        .await
        .unwrap();
        let output = render_json_output(&outcome).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

        assert_eq!(outcome.runners.len(), 2);
        assert_eq!(outcome.metrics.result_count, 2);
        assert_eq!(outcome.metrics.request_counts.list_requests, 1);
        assert_eq!(outcome.metrics.request_counts.detail_requests, 0);
        assert_eq!(outcome.metrics.request_counts.manager_requests, 0);
        assert!(parsed.get("runners").is_some());
        assert!(parsed.get("metrics").is_some());
        assert!(parsed.get("all_runners_fell_back").is_some());

        list.assert_async().await;
    }

    #[tokio::test]
    async fn fetch_summary_rejects_version_filter_before_querying() {
        let conductor = command_test_conductor("http://127.0.0.1:1".to_string());

        let error = match run_cli_query(
            &conductor,
            CliCommand::Fetch { summary: true },
            None,
            Some("17.5"),
            None,
        )
        .await
        {
            Ok(_) => panic!("fetch --summary with --version should fail"),
            Err(error) => error.to_string(),
        };

        assert!(error.contains("fetch --summary cannot be combined with --version"));
    }

    #[test]
    fn validate_cli_args_rejects_fetch_summary_with_version() {
        let args = Args::try_parse_from([
            "gitlab-runner-tui",
            "fetch",
            "--summary",
            "--version",
            "17.5",
        ])
        .unwrap();

        let error = validate_cli_args(&args).unwrap_err().to_string();

        assert!(error.contains("fetch --summary cannot be combined with --version"));
    }

    #[tokio::test]
    async fn rotating_command_returns_filtered_json_envelope() {
        let mut server = Server::new_async().await;
        let mocks = setup_command_runner_mocks(&mut server, Some("prod"), &[1]).await;
        let conductor = command_test_conductor(server.url());

        let outcome = run_cli_query(
            &conductor,
            CliCommand::Rotating { wait: false },
            Some("prod"),
            None,
            None,
        )
        .await
        .unwrap();
        let output = render_json_output(&outcome).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        let runners = parsed["runners"].as_array().unwrap();

        assert_eq!(outcome.runners.len(), 1);
        assert_eq!(outcome.runners[0].id, 1);
        assert_eq!(outcome.metrics.result_count, 1);
        assert_eq!(outcome.metrics.request_counts.detail_requests, 1);
        assert_eq!(outcome.metrics.request_counts.manager_requests, 2);
        assert_eq!(runners.len(), 1);
        assert_eq!(runners[0]["id"], 1);

        for mock in mocks {
            mock.assert_async().await;
        }
    }

    #[tokio::test]
    async fn rotating_wait_completes_for_already_rotated_runner() {
        let mut server = Server::new_async().await;
        let list = server
            .mock("GET", "/api/v4/groups/123/runners")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("per_page".into(), "100".into()),
                Matcher::UrlEncoded("page".into(), "1".into()),
                Matcher::UrlEncoded("tag_list[]".into(), "prod".into()),
            ]))
            .with_status(200)
            .with_body(format!("[{}]", list_response_body(1, "online")))
            .create_async()
            .await;
        let detail = server
            .mock("GET", "/api/v4/runners/1")
            .with_status(200)
            .with_body(detail_response_body(1, "online", &["prod"]))
            .expect(0)
            .create_async()
            .await;
        let managers = server
            .mock("GET", "/api/v4/runners/1/managers")
            .with_status(200)
            .with_body(format!(
                "[{}]",
                manager_response_body_with_times(
                    11,
                    1,
                    "online",
                    "2999-01-01T00:00:00Z",
                    "2999-01-01T00:00:00Z",
                )
            ))
            .create_async()
            .await;
        let conductor = command_test_conductor(server.url());
        let config = AppConfig {
            poll_interval_secs: 1,
            poll_timeout_secs: 1,
            rotation_wait: crate::config::RotationWaitConfig {
                completion_stability_polls: 1,
                ..crate::config::RotationWaitConfig::default()
            },
            ..AppConfig::default()
        };

        let completed = run_rotation_wait(&conductor, &config, Some("prod"), None)
            .await
            .unwrap();

        assert!(completed);
        list.assert_async().await;
        detail.assert_async().await;
        managers.assert_async().await;
    }

    #[test]
    fn builds_default_stale_threshold_for_flames() {
        assert_eq!(
            build_stale_threshold(CliCommand::Flames, None).unwrap(),
            ContactThreshold::OlderThanSecs(3600)
        );
    }

    #[test]
    fn accepts_stale_cutoff_for_flames() {
        let threshold =
            build_stale_threshold(CliCommand::Flames, Some("2026-05-12T11:00:00+01:00")).unwrap();

        assert!(matches!(threshold, ContactThreshold::Since(_)));
    }

    #[test]
    fn rejects_stale_cutoff_for_non_flames_commands() {
        let error = build_stale_threshold(CliCommand::Rotating { wait: false }, Some("11:00"))
            .unwrap_err()
            .to_string();

        assert!(error.contains("--stale-cutoff is only supported with flames"));
    }

    #[test]
    fn rotation_wait_defaults_window_start_to_command_start() {
        let command_started_at = DateTime::parse_from_rfc3339("2026-06-29T12:34:56Z")
            .unwrap()
            .with_timezone(&Utc);

        let options = build_rotation_wait_options(
            &crate::config::RotationWaitConfig::default(),
            command_started_at,
        )
        .unwrap();

        assert_eq!(options.rotation_window_start, command_started_at);
        assert_eq!(options.active_contacted_within_secs, 3600);
        assert_eq!(options.missing_runner_grace_polls, 2);
        assert_eq!(options.completion_stability_polls, 2);
    }

    #[test]
    fn rotation_wait_uses_configured_timezone_window_start() {
        let command_started_at = DateTime::parse_from_rfc3339("2026-06-29T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let config = crate::config::RotationWaitConfig {
            timezone: Some("Europe/London".to_string()),
            rotation_window_start: Some("00:00".to_string()),
            ..crate::config::RotationWaitConfig::default()
        };

        let options = build_rotation_wait_options(&config, command_started_at).unwrap();

        assert_eq!(
            options.rotation_window_start,
            DateTime::parse_from_rfc3339("2026-06-28T23:00:00Z")
                .unwrap()
                .with_timezone(&Utc)
        );
    }

    #[test]
    fn rejects_blank_stale_cutoff_for_cli() {
        let error = build_stale_threshold(CliCommand::Flames, Some(" "))
            .unwrap_err()
            .to_string();

        assert!(error.contains("--stale-cutoff cannot be blank"));
    }

    #[test]
    fn builds_empty_filters_when_tags_missing_or_blank() {
        assert_eq!(build_runner_filters(None, None), RunnerFilters::default());
        assert_eq!(
            build_runner_filters(Some(" , , "), None),
            RunnerFilters::default()
        );
    }

    #[test]
    fn builds_trimmed_tag_filters() {
        let filters = build_runner_filters(Some(" alm, production ,, linux "), None);
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
        let filters = build_runner_filters(Some("prod, staging,prod,qa"), None);
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
        let filters = build_runner_filters(Some("\tprod,\nqa,  staging "), None);
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
    fn builds_version_filter() {
        let filters = build_runner_filters(None, Some("16.11"));
        assert_eq!(filters.version_prefix, Some("16.11".to_string()));
        assert_eq!(filters.tag_list, None);
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
            command: None,
            host: None,
            config: None,
            dotenv: None,
            tags: None,
            version_filter: None,
            stale_cutoff: None,
            demo: false,
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
    fn command_mode_requires_token_when_not_configured() {
        let args = Args {
            command: Some(CliCommand::Fetch { summary: false }),
            host: None,
            config: None,
            dotenv: None,
            tags: None,
            version_filter: None,
            stale_cutoff: None,
            demo: false,
        };

        let error = resolve_runtime_settings_with_env(&args, AppConfig::default(), None, None)
            .unwrap_err()
            .to_string();

        assert!(error.contains("GITLAB_TOKEN must be set"));
    }

    #[test]
    fn command_mode_allows_missing_runner_targets() {
        let args = Args {
            command: Some(CliCommand::Fetch { summary: false }),
            host: None,
            config: None,
            dotenv: None,
            tags: None,
            version_filter: None,
            stale_cutoff: None,
            demo: false,
        };

        let (host, token, config) = resolve_runtime_settings_with_env(
            &args,
            AppConfig::default(),
            None,
            Some("glpat-test".to_string()),
        )
        .unwrap();

        assert_eq!(host, "https://gitlab.com");
        assert_eq!(token, "glpat-test");
        assert!(config.runner_targets.is_empty());
    }

    #[test]
    fn command_mode_rejects_invalid_rotation_wait_config() {
        let args = Args {
            command: Some(CliCommand::Rotating { wait: true }),
            host: None,
            config: None,
            dotenv: None,
            tags: Some("prod".to_string()),
            version_filter: None,
            stale_cutoff: None,
            demo: false,
        };
        let config = AppConfig {
            rotation_wait: crate::config::RotationWaitConfig {
                active_contacted_within_secs: 0,
                ..crate::config::RotationWaitConfig::default()
            },
            ..AppConfig::default()
        };

        let error =
            resolve_runtime_settings_with_env(&args, config, None, Some("glpat-test".to_string()))
                .unwrap_err()
                .to_string();

        assert!(error.contains("active contact threshold"));
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
        let conductor = Conductor::new_with_mode(
            client,
            RunnerDiscoveryMode::ConfiguredTargets,
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
