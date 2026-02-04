use crate::tui::app::{App, ResultsViewType};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Cell, List, ListItem, Paragraph, Row, Table},
    Frame,
};

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
    } else {
        "GitLab Runner TUI".to_string()
    };
    let title = Paragraph::new(title).block(Block::default().borders(Borders::ALL));
    frame.render_widget(title, chunks[0]);

    // Content based on mode
    match app.mode {
        crate::tui::app::AppMode::CommandSelection => {
            render_command_selection(app, frame, chunks[1])
        }
        crate::tui::app::AppMode::FilterInput => render_filter_input(app, frame, chunks[1]),
        crate::tui::app::AppMode::ResultsView => render_results(app, frame, chunks[1]),
        crate::tui::app::AppMode::Help => render_help_view(app, frame, chunks[1]),
    };

    // Status bar with context-sensitive help
    let status_text = if app.error_message.is_some() {
        "Press Esc to dismiss error and go back"
    } else {
        match app.mode {
            crate::tui::app::AppMode::CommandSelection => {
                "↑/↓: Navigate | Enter: Select | ?: Help | q: Quit"
            }
            crate::tui::app::AppMode::FilterInput => {
                "Enter: Search | Esc: Back | Type to filter by tags"
            }
            crate::tui::app::AppMode::ResultsView => "↑/↓: Scroll | Esc: Back | q: Quit",
            crate::tui::app::AppMode::Help => "Press any key to close help",
        }
    };
    let status = Paragraph::new(status_text).block(Block::default().borders(Borders::ALL));
    frame.render_widget(status, chunks[2]);
}

fn render_command_selection(app: &mut App, frame: &mut Frame, area: ratatui::layout::Rect) {
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

fn render_filter_input(app: &App, frame: &mut Frame, area: ratatui::layout::Rect) {
    let input = Paragraph::new(app.input_buffer.as_str())
        .style(Style::default().fg(Color::Yellow))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Filter Input (Press Enter to search)"),
        );
    frame.render_widget(input, area);
}

fn render_results(app: &mut App, frame: &mut Frame, area: ratatui::layout::Rect) {
    // Check for error message first
    if let Some(error) = &app.error_message {
        render_error(error, frame, area);
        return;
    }

    match app.results_view_type {
        ResultsViewType::Runners => render_runners_table(app, frame, area),
        ResultsViewType::Workers => render_workers_table(app, frame, area),
        ResultsViewType::HealthCheck => render_health_check(app, frame, area),
    }
}

fn render_error(error: &str, frame: &mut Frame, area: ratatui::layout::Rect) {
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

fn render_runners_table(app: &mut App, frame: &mut Frame, area: ratatui::layout::Rect) {
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

    let rows = app.runners.iter().map(|runner| {
        let status_style = match runner.status.as_str() {
            "online" => Style::default().fg(Color::Green),
            "offline" => Style::default().fg(Color::Red),
            "stale" => Style::default().fg(Color::Yellow),
            _ => Style::default().fg(Color::Gray),
        };
        Row::new(vec![
            Cell::from(runner.id.to_string()),
            Cell::from(runner.runner_type.clone()),
            Cell::from(runner.status.clone()).style(status_style),
            Cell::from(runner.version.clone().unwrap_or_else(|| "-".to_string())),
            Cell::from(runner.tag_list.join(", ")),
            Cell::from(runner.managers.len().to_string()),
            Cell::from(runner.ip_address.clone().unwrap_or_else(|| "-".to_string())),
        ])
    });

    let table = Table::new(
        rows,
        [
            Constraint::Length(10),     // ID
            Constraint::Length(15),     // Type
            Constraint::Length(10),     // Status
            Constraint::Length(10),     // Version
            Constraint::Percentage(30), // Tags
            Constraint::Length(10),     // Managers
            Constraint::Length(15),     // IP
        ],
    )
    .header(header)
    .highlight_style(Style::default().bg(Color::DarkGray))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!("Results ({} runners)", app.runners.len())),
    );

    frame.render_stateful_widget(table, area, &mut app.table_state);
}

fn render_workers_table(app: &mut App, frame: &mut Frame, area: ratatui::layout::Rect) {
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
        let status_style = match row.manager.status.as_str() {
            "online" => Style::default().fg(Color::Green),
            "offline" => Style::default().fg(Color::Red),
            "stale" => Style::default().fg(Color::Yellow),
            _ => Style::default().fg(Color::Gray),
        };
        Row::new(vec![
            Cell::from(row.runner_id.to_string()),
            Cell::from(row.runner_tags.join(", ")),
            Cell::from(row.manager.id.to_string()),
            Cell::from(row.manager.system_id.clone()),
            Cell::from(row.manager.status.clone()).style(status_style),
            Cell::from(
                row.manager
                    .version
                    .clone()
                    .unwrap_or_else(|| "-".to_string()),
            ),
            Cell::from(
                row.manager
                    .contacted_at
                    .clone()
                    .unwrap_or_else(|| "Never".to_string()),
            ),
            Cell::from(
                row.manager
                    .ip_address
                    .clone()
                    .unwrap_or_else(|| "-".to_string()),
            ),
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

fn render_health_check(app: &mut App, frame: &mut Frame, area: ratatui::layout::Rect) {
    use ratatui::layout::Layout;

    // Split area: summary at top, table below
    let chunks = Layout::default()
        .direction(ratatui::layout::Direction::Vertical)
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
    render_runners_table_in_area(app, frame, chunks[1]);
}

fn render_runners_table_in_area(app: &mut App, frame: &mut Frame, area: ratatui::layout::Rect) {
    let header = Row::new(vec![
        Cell::from("ID"),
        Cell::from("Type"),
        Cell::from("Tags"),
        Cell::from("Version"),
        Cell::from("Status"),
        Cell::from("Managers"),
        Cell::from("IP"),
    ])
    .style(
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    );

    let rows = app.runners.iter().map(|runner| {
        let status_style = match runner.status.as_str() {
            "online" => Style::default().fg(Color::Green),
            "offline" => Style::default().fg(Color::Red),
            "stale" => Style::default().fg(Color::Yellow),
            _ => Style::default().fg(Color::Gray),
        };
        Row::new(vec![
            Cell::from(runner.id.to_string()),
            Cell::from(runner.runner_type.clone()),
            Cell::from(runner.tag_list.join(", ")),
            Cell::from(runner.version.clone().unwrap_or_else(|| "-".to_string())),
            Cell::from(runner.status.clone()).style(status_style),
            Cell::from(runner.managers.len().to_string()),
            Cell::from(runner.ip_address.clone().unwrap_or_else(|| "-".to_string())),
        ])
    });

    let table = Table::new(
        rows,
        [
            Constraint::Length(8),
            Constraint::Length(14),
            Constraint::Percentage(25),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(15),
        ],
    )
    .header(header)
    .highlight_style(Style::default().bg(Color::DarkGray))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!("Runners ({})", app.runners.len())),
    );

    frame.render_stateful_widget(table, area, &mut app.table_state);
}

fn render_help_view(_app: &mut App, frame: &mut Frame, area: ratatui::layout::Rect) {
    let help_text = vec![
        "GitLab Runner TUI - Help",
        "---------",
        "",
        "Navigation:",
        "  ↑/↓ or k/j    Navigate commands / Scroll results",
        "  Enter         Select command / Execute search",
        "  Tab           Next filter field",
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
        "",
        "Filters (in filter mode):",
        "  Tag List      Comma-separated tags (e.g., alm,prod)",
        "  Status        online, offline, stale, never_contacted",
        "  Version       Version prefix (e.g., 17, 18.5)",
        "  Type          instance_type, group_type, project_type",
        "  Paused        true or false",
        "",
        "Press any key to close help",
    ];

    let items: Vec<ListItem> = help_text.into_iter().map(ListItem::new).collect();
    let list = List::new(items).block(Block::default().borders(Borders::ALL).title("Help"));
    frame.render_widget(list, area);
}
