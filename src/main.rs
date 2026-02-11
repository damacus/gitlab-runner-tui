mod client;
mod conductor;
mod models;
mod tui;

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
use std::io;
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

    let host = args
        .host
        .unwrap_or_else(|| "https://gitlab.com".to_string());

    let token = args
        .token
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
                Event::Key(key) => app.handle_key(key).await,
                Event::Tick => app.tick(),
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
