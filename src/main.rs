mod client;
mod conductor;
mod config;
mod models;
mod tui;
mod utils; // Ensure this file exists even if empty

use anyhow::Result;
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

#[derive(Parser, Debug)]
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
        .expect("GITLAB_TOKEN must be set via environment variable, --token flag, or config.toml");

    let client = GitLabClient::new(host, token)?;
    let conductor = Conductor::new(client);

    if args.watch {
        return run_headless(conductor, config, &args.command, args.tags.as_deref()).await;
    }

    let mut app = App::new(conductor, config);

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
                Event::Key(key) => {
                    match key.code {
                        crossterm::event::KeyCode::Char('?') => {
                            if app.mode != tui::app::AppMode::Help {
                                app.mode = tui::app::AppMode::Help;
                            } else {
                                app.mode = tui::app::AppMode::CommandSelection; // Or back to previous?
                            }
                        }
                        crossterm::event::KeyCode::Char('q') => app.should_quit = true,
                        crossterm::event::KeyCode::Char('p')
                            if app.mode == tui::app::AppMode::ResultsView =>
                        {
                            app.toggle_polling();
                        }
                        crossterm::event::KeyCode::Up | crossterm::event::KeyCode::Char('k') => {
                            match app.mode {
                                tui::app::AppMode::CommandSelection => app.previous_command(),
                                tui::app::AppMode::ResultsView => app.previous_result(),
                                _ => {}
                            }
                        }
                        crossterm::event::KeyCode::Down | crossterm::event::KeyCode::Char('j') => {
                            match app.mode {
                                tui::app::AppMode::CommandSelection => app.next_command(),
                                tui::app::AppMode::ResultsView => app.next_result(),
                                _ => {}
                            }
                        }
                        crossterm::event::KeyCode::Enter => match app.mode {
                            tui::app::AppMode::CommandSelection => app.select_command().await,
                            tui::app::AppMode::FilterInput => app.execute_search().await,
                            _ => {}
                        },
                        crossterm::event::KeyCode::Esc => match app.mode {
                            tui::app::AppMode::CommandSelection => app.should_quit = true,
                            tui::app::AppMode::FilterInput => {
                                app.error_message = None;
                                app.mode = tui::app::AppMode::CommandSelection;
                            }
                            tui::app::AppMode::ResultsView => {
                                app.error_message = None;
                                app.mode = tui::app::AppMode::CommandSelection;
                            }
                            _ => app.mode = tui::app::AppMode::CommandSelection,
                        },
                        // FilterInput text entry
                        crossterm::event::KeyCode::Char(c)
                            if app.mode == tui::app::AppMode::FilterInput =>
                        {
                            app.input_buffer.push(c);
                        }
                        crossterm::event::KeyCode::Backspace
                            if app.mode == tui::app::AppMode::FilterInput =>
                        {
                            app.input_buffer.pop();
                        }
                        _ => {}
                    }
                }
                Event::Tick => app.tick().await,
            }
        }

        if app.should_quit {
            break;
        }
    }

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
    let poll_interval = std::time::Duration::from_secs(config.poll_interval_secs);
    let started_at = Instant::now();
    let mut iteration = 0u64;

    loop {
        iteration += 1;
        let elapsed = started_at.elapsed().as_secs();

        let mut filters = RunnerFilters::default();
        if let Some(tag_str) = tags {
            filters.tag_list = Some(tag_str.split(',').map(|s| s.trim().to_string()).collect());
        }

        let result = match command {
            "fetch" => conductor.fetch_runners(filters).await,
            "switch" => conductor.list_offline_runners(filters).await,
            "flames" => conductor.list_uncontacted_runners(filters, 3600).await,
            "empty" => conductor.list_runners_without_managers(filters).await,
            "rotate" => conductor.detect_rotating_runners(filters).await,
            other => {
                eprintln!("Unknown command: {}", other);
                std::process::exit(1);
            }
        };

        match result {
            Ok(runners) => {
                println!(
                    "[{:02}:{:02}] Poll #{} — {} runners matched (command: {})",
                    elapsed / 60,
                    elapsed % 60,
                    iteration,
                    runners.len(),
                    command,
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

                if runners.is_empty() && command == "rotate" {
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
