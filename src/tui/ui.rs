use crate::tui::app::{App, AppMode, ResultsViewType};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Cell, List, ListItem, Paragraph, Row, Table},
    Frame,
};

const ONLINE_STYLE: Style = Style::new().fg(Color::Green);
const OFFLINE_STYLE: Style = Style::new().fg(Color::Red);
const STALE_STYLE: Style = Style::new().fg(Color::Yellow);
const DEFAULT_STYLE: Style = Style::new().fg(Color::Gray);

fn status_style(status: &str) -> Style {
    match status {
        "online" => ONLINE_STYLE,
        "offline" => OFFLINE_STYLE,
        "stale" => STALE_STYLE,
        _ => DEFAULT_STYLE,
    }
}

fn dash_or(value: &Option<String>) -> &str {
    value.as_deref().unwrap_or("-")
}

pub fn render(app: &mut App, frame: &mut Frame) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(3),
        ])
        .split(frame.size());

    // Header
    let title = if app.is_loading {
        format!("GitLab Runner TUI {} Loading...", app.spinner_char())
    } else if app.polling_active {
        let elapsed = app.poll_elapsed_secs();
        let timeout = app.config.poll_timeout_secs;
        format!(
            "GitLab Runner TUI  ⟳ Polling ({:02}:{:02} / {:02}:{:02})",
            elapsed / 60,
            elapsed % 60,
            timeout / 60,
            timeout % 60
        )
    } else {
        "GitLab Runner TUI".to_string()
    };
    let title = Paragraph::new(title).block(Block::default().borders(Borders::ALL));
    frame.render_widget(title, chunks[0]);

    // Content based on mode
    match app.mode {
        AppMode::CommandSelection => render_command_selection(app, frame, chunks[1]),
        AppMode::FilterInput => render_filter_input(app, frame, chunks[1]),
        AppMode::ResultsView => render_results(app, frame, chunks[1]),
        AppMode::Help => render_help_view(app, frame, chunks[1]),
    };

    // Status bar with context-sensitive help
    let status_text = if app.error_message.is_some() {
        "Press Esc to dismiss error and go back"
    } else {
        match app.mode {
            AppMode::CommandSelection => "↑/↓: Navigate | Enter: Select | ?: Help | q: Quit",
            AppMode::FilterInput => "Enter: Search | Esc: Back | Type to filter by tags",
            AppMode::ResultsView => {
                if app.polling_active {
                    "↑/↓: Scroll | p: Stop polling | Esc: Back | q: Quit"
                } else {
                    "↑/↓: Scroll | p: Start polling | Esc: Back | q: Quit"
                }
            }
            AppMode::Help => "Press any key to close help",
        }
    };
    let status = Paragraph::new(status_text).block(Block::default().borders(Borders::ALL));
    frame.render_widget(status, chunks[2]);
}

fn render_command_selection(app: &mut App, frame: &mut Frame, area: Rect) {
    let items: Vec<ListItem> = app
        .commands
        .iter()
        .map(|cmd| ListItem::new(cmd.to_string()))
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Select Command"),
        )
        .highlight_style(
            Style::default()
                .bg(Color::Yellow)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(">> ");

    let mut state = ratatui::widgets::ListState::default();
    state.select(Some(app.selected_command_index));

    frame.render_stateful_widget(list, area, &mut state);
}

fn render_filter_input(app: &App, frame: &mut Frame, area: Rect) {
    let (text, style) = if app.input_buffer.is_empty() {
        (
            "Enter comma-separated tags (e.g., prod, linux)...",
            Style::default().fg(Color::DarkGray),
        )
    } else {
        (
            app.input_buffer.as_str(),
            Style::default().fg(Color::Yellow),
        )
    };

    let input = Paragraph::new(text).style(style).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Filter Input (Press Enter to search)"),
    );
    frame.render_widget(input, area);

    // Make the cursor visible and set its position
    frame.set_cursor(
        // Put cursor past the end of the input text
        area.x + app.input_buffer.chars().count() as u16 + 1,
        // Move one line down, from the border to the input line
        area.y + 1,
    );
}

fn render_results(app: &mut App, frame: &mut Frame, area: Rect) {
    // Check for error message first
    if let Some(error) = &app.error_message {
        render_error(error, frame, area);
        return;
    }

    match app.results_view_type {
        ResultsViewType::Runners => render_runners_table(app, frame, area),
        ResultsViewType::Workers => render_workers_table(app, frame, area),
        ResultsViewType::HealthCheck => render_health_check(app, frame, area),
        ResultsViewType::Rotation => render_rotation_table(app, frame, area),
    }
}

fn render_error(error: &str, frame: &mut Frame, area: Rect) {
    let error_detail = format!("  {}", error);
    let error_text: Vec<String> = vec![
        "".to_string(),
        "  ✗ Error occurred".to_string(),
        "".to_string(),
        error_detail,
        "".to_string(),
        "  Troubleshooting:".to_string(),
        "  • Check GITLAB_HOST and GITLAB_TOKEN are set correctly".to_string(),
        "  • Verify network connectivity to GitLab".to_string(),
        "  • Ensure your token has 'read_api' scope".to_string(),
        "".to_string(),
    ];

    let items: Vec<ListItem> = error_text
        .into_iter()
        .map(|line| ListItem::new(line).style(Style::default().fg(Color::Red)))
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Error")
            .border_style(Style::default().fg(Color::Red)),
    );

    frame.render_widget(list, area);
}

fn render_runners_table(app: &mut App, frame: &mut Frame, area: Rect) {
    render_runners_table_impl(
        app,
        frame,
        area,
        format!("Results ({} runners)", app.runners.len()),
    );
}

fn render_workers_table(app: &mut App, frame: &mut Frame, area: Rect) {
    if app.manager_rows.is_empty() {
        let msg = Paragraph::new(
            "\n  No workers found.\n\n  Press 'Esc' to go back or adjust your filter tags.",
        )
        .style(Style::default().fg(Color::Gray))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Workers (0 managers)"),
        );
        frame.render_widget(msg, area);
        return;
    }

    let header = Row::new(vec![
        Cell::from("Runner ID"),
        Cell::from("Tags"),
        Cell::from("Manager ID"),
        Cell::from("System ID"),
        Cell::from("Status"),
        Cell::from("Version"),
        Cell::from("Contacted"),
        Cell::from("IP"),
    ])
    .style(
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    );

    let rows = app.manager_rows.iter().map(|row| {
        Row::new(vec![
            Cell::from(row.runner_id_str.as_str()),
            Cell::from(row.runner_tags_str.as_str()),
            Cell::from(row.manager_id_str.as_str()),
            Cell::from(row.manager.system_id.as_str()),
            Cell::from(row.manager.status.as_str()).style(status_style(&row.manager.status)),
            Cell::from(dash_or(&row.manager.version)),
            Cell::from(
                row.manager
                    .contacted_at
                    .as_deref()
                    .unwrap_or("Never")
            ),
            Cell::from(dash_or(&row.manager.ip_address)),
        ])
    });

    let table = Table::new(
        rows,
        [
            Constraint::Length(10),     // Runner ID
            Constraint::Percentage(20), // Tags
            Constraint::Length(12),     // Manager ID
            Constraint::Percentage(15), // System ID
            Constraint::Length(10),     // Status
            Constraint::Length(10),     // Version
            Constraint::Length(20),     // Contacted
            Constraint::Length(15),     // IP
        ],
    )
    .header(header)
    .highlight_style(Style::default().bg(Color::DarkGray))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!("Workers ({} managers)", app.manager_rows.len())),
    );

    frame.render_stateful_widget(table, area, &mut app.table_state);
}

fn render_health_check(app: &mut App, frame: &mut Frame, area: Rect) {
    // Split area: summary at top, table below
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(5), Constraint::Min(10)])
        .split(area);

    // Render health summary
    if let Some(ref summary) = app.health_summary {
        let percentage = summary.percentage();
        let is_healthy = summary.is_healthy();

        let status_symbol = if is_healthy { "✓" } else { "✗" };
        let status_color = if is_healthy { Color::Green } else { Color::Red };

        let summary_text = format!(
            "{} {} of {} runners online ({:.1}%)",
            status_symbol, summary.online_count, summary.total_count, percentage
        );

        let health_paragraph = Paragraph::new(summary_text)
            .style(
                Style::default()
                    .fg(status_color)
                    .add_modifier(Modifier::BOLD),
            )
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Health Check Summary")
                    .border_style(Style::default().fg(status_color)),
            );

        frame.render_widget(health_paragraph, chunks[0]);
    }

    // Render runners table in remaining space
    render_runners_table_impl(
        app,
        frame,
        chunks[1],
        format!("Runners ({})", app.runners.len()),
    );
}

fn render_runners_table_impl(app: &mut App, frame: &mut Frame, area: Rect, title: String) {
    if app.runners.is_empty() {
        let msg = Paragraph::new(
            "\n  No runners found.\n\n  Press 'Esc' to go back or adjust your filter tags.",
        )
        .style(Style::default().fg(Color::Gray))
        .block(Block::default().borders(Borders::ALL).title(title));
        frame.render_widget(msg, area);
        return;
    }

    let header = Row::new(vec![
        Cell::from("ID"),
        Cell::from("Type"),
        Cell::from("Status"),
        Cell::from("Version"),
        Cell::from("Tags"),
        Cell::from("Managers"),
        Cell::from("IP"),
    ])
    .style(
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    );

    let rows = app.runners.iter().map(|view| {
        Row::new(vec![
            Cell::from(view.id_str.as_str()),
            Cell::from(view.runner.runner_type.as_str()),
            Cell::from(view.runner.status.as_str()).style(status_style(&view.runner.status)),
            Cell::from(dash_or(&view.runner.version)),
            Cell::from(view.tags_str.as_str()),
            Cell::from(view.managers_len_str.as_str()),
            Cell::from(dash_or(&view.runner.ip_address)),
        ])
    });

    let table = Table::new(
        rows,
        [
            Constraint::Length(10),     // ID
            Constraint::Length(15),     // Type
            Constraint::Length(10),     // Status
            Constraint::Length(10),     // Version
            Constraint::Percentage(25), // Tags
            Constraint::Length(10),     // Managers
            Constraint::Length(15),     // IP
        ],
    )
    .header(header)
    .highlight_style(Style::default().bg(Color::DarkGray))
    .block(Block::default().borders(Borders::ALL).title(title));

    frame.render_stateful_widget(table, area, &mut app.table_state);
}

fn render_rotation_table(app: &mut App, frame: &mut Frame, area: Rect) {
    if app.runners.is_empty() {
        let msg = Paragraph::new(
            "\n  No rotation detected - all runners have a single manager.\n\n  Press 'Esc' to go back or adjust your filter tags.",
        )
        .style(Style::default().fg(Color::Gray))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Rotation Status"),
        );
        frame.render_widget(msg, area);
        return;
    }

    let header = Row::new(vec![
        Cell::from("Runner ID"),
        Cell::from("Tags"),
        Cell::from("Mgrs"),
        Cell::from("Old System"),
        Cell::from("Old Ver"),
        Cell::from("Old Status"),
        Cell::from("New System"),
        Cell::from("New Ver"),
        Cell::from("New Status"),
    ])
    .style(
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    );

    let rows = app.runners.iter().map(|view| {
        // ⚡ Bolt: find min and max without cloning or sorting, O(N) instead of O(N log N) with allocs
        let oldest = view.runner.managers.iter().min_by_key(|m| &m.created_at);
        let newest = if view.runner.managers.len() > 1 {
            view.runner.managers.iter().max_by_key(|m| &m.created_at)
        } else {
            None
        };

        let old_system = oldest.map(|m| m.system_id.as_str()).unwrap_or("-");
        let old_ver = oldest.and_then(|m| m.version.as_deref()).unwrap_or("-");
        let old_status = oldest.map(|m| m.status.as_str()).unwrap_or("-");

        let new_system = newest.map(|m| m.system_id.as_str()).unwrap_or("-");
        let new_ver = newest.and_then(|m| m.version.as_deref()).unwrap_or("-");
        let new_status = newest.map(|m| m.status.as_str()).unwrap_or("-");

        Row::new(vec![
            Cell::from(view.id_str.as_str()),
            Cell::from(view.tags_str.as_str()),
            Cell::from(view.managers_len_str.as_str()),
            Cell::from(old_system),
            Cell::from(old_ver),
            Cell::from(old_status).style(status_style(old_status)),
            Cell::from(new_system),
            Cell::from(new_ver),
            Cell::from(new_status).style(status_style(new_status)),
        ])
    });

    let table = Table::new(
        rows,
        [
            Constraint::Length(10),     // Runner ID
            Constraint::Percentage(15), // Tags
            Constraint::Length(5),      // Mgrs
            Constraint::Percentage(12), // Old System
            Constraint::Length(10),     // Old Ver
            Constraint::Length(10),     // Old Status
            Constraint::Percentage(12), // New System
            Constraint::Length(10),     // New Ver
            Constraint::Length(10),     // New Status
        ],
    )
    .header(header)
    .highlight_style(Style::default().bg(Color::DarkGray))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!("Rotating Runners ({} detected)", app.runners.len())),
    );

    frame.render_stateful_widget(table, area, &mut app.table_state);
}

fn render_help_view(_app: &mut App, frame: &mut Frame, area: Rect) {
    let help_text = vec![
        "GitLab Runner TUI - Help",
        "---------",
        "",
        "Navigation:",
        "  ↑/↓ or k/j    Navigate commands / Scroll results",
        "  Enter         Select command / Execute search",
        "  Esc           Back / Cancel",
        "  ?             Toggle this help",
        "  q             Quit application",
        "",
        "Commands:",
        "  fetch         Fetch GitLab Runner details",
        "  lights        Check if runners are online (health check)",
        "  switch        List runners with offline managers",
        "  workers       Show runner managers (flattened view)",
        "  flames        List runners not contacted recently",
        "  empty         List runners with no managers",
        "  rotate        Detect runners with multiple managers (rotation)",
        "",
        "Polling (in results view):",
        "  p             Toggle auto-refresh polling",
        "",
        "Filter (in filter mode):",
        "  Tags          Comma-separated tags (e.g., alm,prod)",
        "",
        "Press any key to close help",
    ];

    let items: Vec<ListItem> = help_text.into_iter().map(ListItem::new).collect();
    let list = List::new(items).block(Block::default().borders(Borders::ALL).title("Help"));
    frame.render_widget(list, area);
}
