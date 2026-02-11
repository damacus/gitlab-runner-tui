mod client;
mod conductor;
mod models;
mod tui;
mod utils; // Ensure this file exists even if empty

use anyhow::Result;
use clap::Parser;
use client::GitLabClient;
use conductor::Conductor;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{env, io};
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
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let args = Args::parse();

    // Setup logging
    let file_appender = tracing_appender::rolling::daily("logs", "gitlab-runner-tui.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
    tracing_subscriber::fmt()
        .with_writer(non_blocking)
        .with_ansi(false)
        .init();

    // GITLAB_HOST defaults to gitlab.com if not provided
    let host = args
        .host
        .or_else(|| env::var("GITLAB_HOST").ok())
        .unwrap_or_else(|| "https://gitlab.com".to_string());

    // GITLAB_TOKEN is required
    let token = args
        .token
        .or_else(|| env::var("GITLAB_TOKEN").ok())
        .expect("GITLAB_TOKEN must be set via environment variable or --token flag");

    let client = GitLabClient::new(host, token)?;
    let conductor = Conductor::new(client);
    let mut app = App::new(conductor);

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
