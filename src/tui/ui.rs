use crate::tui::{
    app::{detail_layout_mode, App, AppMode, DetailLayoutMode, ResultsViewType},
    styles,
};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::{Cell, List, ListItem, Paragraph, Row, Table, Tabs, Wrap},
    Frame,
};

fn dash_or(value: &Option<String>) -> String {
    value.as_deref().unwrap_or("-").to_string()
}

pub fn render(app: &mut App, frame: &mut Frame) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(2),
        ])
        .split(frame.size());

    render_header(app, frame, chunks[0]);
    render_tabs(app, frame, chunks[1]);
    render_filter_bar(app, frame, chunks[2]);

    match app.mode {
        AppMode::Help => render_help_view(frame, chunks[3]),
        _ => render_content(app, frame, chunks[3]),
    }

    render_status_bar(app, frame, chunks[4]);
}

fn render_header(app: &App, frame: &mut Frame, area: Rect) {
    let title = if app.is_loading {
        format!(
            "GitLab Runner TUI {} {}",
            app.spinner_char(),
            app.active_tab().loading_label()
        )
    } else if app.polling_active {
        let elapsed = app.poll_elapsed_secs();
        let timeout = app.config.poll_timeout_secs;
        format!(
            "GitLab Runner TUI  ⟳ Polling {:02}:{:02} / {:02}:{:02}",
            elapsed / 60,
            elapsed % 60,
            timeout / 60,
            timeout % 60
        )
    } else {
        "GitLab Runner TUI".to_string()
    };

    let header = Paragraph::new(title)
        .style(styles::app_title_style())
        .block(styles::block("Dashboard"));
    frame.render_widget(header, area);
}

fn render_tabs(app: &App, frame: &mut Frame, area: Rect) {
    let titles: Vec<Line> = app
        .tabs
        .iter()
        .map(|tab| {
            Line::from(vec![
                Span::styled(format!("{} ", tab.shortcut()), styles::muted_style()),
                Span::raw(tab.title()),
            ])
        })
        .collect();

    let tabs = Tabs::new(titles)
        .select(app.active_tab_index)
        .style(styles::tab_style())
        .highlight_style(styles::active_tab_style())
        .divider("|")
        .block(styles::block("Views"));

    frame.render_widget(tabs, area);
}

fn render_filter_bar(app: &App, frame: &mut Frame, area: Rect) {
    let title = if app.mode == AppMode::FilterInput {
        "Filter Tags (focused)"
    } else {
        "Filter Tags"
    };

    let (text, style) = if app.filter_input.is_empty() {
        (
            "Type tags like prod,linux and press Enter to fetch the active tab",
            styles::muted_style(),
        )
    } else {
        (app.filter_input.as_str(), styles::accent_style())
    };

    let block = if app.mode == AppMode::FilterInput {
        styles::focused_block(title)
    } else {
        styles::block(title)
    };

    let paragraph = Paragraph::new(text).style(style).block(block);
    frame.render_widget(paragraph, area);

    if app.mode == AppMode::FilterInput {
        frame.set_cursor(
            area.x + app.filter_input.chars().count() as u16 + 1,
            area.y + 1,
        );
    }
}

fn render_content(app: &mut App, frame: &mut Frame, area: Rect) {
    if let Some(error) = &app.error_message {
        render_error(error, frame, area);
        return;
    }

    if !app.has_loaded_active_tab() {
        let prompt = Paragraph::new(format!(
            "Press Enter or r to load the {} tab using the current tag filter.",
            app.active_tab().title()
        ))
        .block(styles::block(app.current_tab_title()))
        .wrap(Wrap { trim: true });
        frame.render_widget(prompt, area);
        return;
    }

    match app.current_results_view_type() {
        ResultsViewType::HealthCheck => render_health_tab(app, frame, area),
        ResultsViewType::Workers => render_workers_tab(app, frame, area),
        ResultsViewType::Rotation => render_rotating_tab(app, frame, area),
        ResultsViewType::Runners => render_runners_tab(app, frame, area),
    }
}

fn render_error(error: &str, frame: &mut Frame, area: Rect) {
    let lines = vec![
        Line::from("Error occurred while loading the current tab."),
        Line::from(""),
        Line::from(error.to_string()),
        Line::from(""),
        Line::from("Check GITLAB_HOST, GITLAB_TOKEN, and network connectivity."),
        Line::from("Press Esc to dismiss the error."),
    ];

    let paragraph = Paragraph::new(lines)
        .block(styles::error_block("Error"))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn render_health_tab(app: &mut App, frame: &mut Frame, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(5), Constraint::Min(1)])
        .split(area);

    render_health_summary(app, frame, chunks[0]);
    render_runners_tab(app, frame, chunks[1]);
}

fn render_health_summary(app: &App, frame: &mut Frame, area: Rect) {
    let text = if let Some(summary) = &app.health_summary {
        let symbol = if summary.is_healthy() { "✓" } else { "✗" };
        format!(
            "{} {} of {} runners online ({:.1}%)",
            symbol,
            summary.online_count,
            summary.total_count,
            summary.percentage()
        )
    } else {
        "No health data loaded yet.".to_string()
    };

    let paragraph = Paragraph::new(text)
        .style(styles::accent_style())
        .block(styles::block("Health Summary"));
    frame.render_widget(paragraph, area);
}

fn render_runners_tab(app: &mut App, frame: &mut Frame, area: Rect) {
    render_runner_like_tab(app, frame, area, false);
}

fn render_rotating_tab(app: &mut App, frame: &mut Frame, area: Rect) {
    render_runner_like_tab(app, frame, area, true);
}

fn render_runner_like_tab(app: &mut App, frame: &mut Frame, area: Rect, rotating: bool) {
    if app.runners.is_empty() {
        let paragraph = Paragraph::new(app.active_tab().empty_label())
            .style(styles::muted_style())
            .block(styles::block(app.current_tab_title()))
            .wrap(Wrap { trim: true });
        frame.render_widget(paragraph, area);
        return;
    }

    match detail_layout_mode(area.width, area.height) {
        DetailLayoutMode::SidePanel => {
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(66), Constraint::Percentage(34)])
                .split(area);
            render_runner_table(app, frame, chunks[0], rotating);
            render_runner_detail(app, frame, chunks[1]);
        }
        DetailLayoutMode::BottomPanel => {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(1), Constraint::Length(10)])
                .split(area);
            render_runner_table(app, frame, chunks[0], rotating);
            render_runner_detail(app, frame, chunks[1]);
        }
        DetailLayoutMode::Compact => {
            render_runner_table(app, frame, area, rotating);
        }
    }
}

fn render_runner_table(app: &mut App, frame: &mut Frame, area: Rect, rotating: bool) {
    let header = if rotating {
        Row::new(vec![
            Cell::from("ID"),
            Cell::from("Tags"),
            Cell::from("Mgrs"),
            Cell::from("Old"),
            Cell::from("Old Ver"),
            Cell::from("New"),
            Cell::from("New Ver"),
            Cell::from("Status"),
        ])
    } else {
        Row::new(vec![
            Cell::from("ID"),
            Cell::from("Type"),
            Cell::from("Status"),
            Cell::from("Version"),
            Cell::from("Tags"),
            Cell::from("Mgrs"),
            Cell::from("IP"),
        ])
    }
    .style(styles::table_header_style());

    let rows = app.runners.iter().map(|runner| {
        if rotating {
            let oldest = runner
                .managers
                .iter()
                .min_by_key(|manager| &manager.created_at);
            let newest = if runner.managers.len() > 1 {
                runner
                    .managers
                    .iter()
                    .max_by_key(|manager| &manager.created_at)
            } else {
                None
            };

            let overall_status = newest
                .map(|manager| manager.status.as_str())
                .or_else(|| oldest.map(|manager| manager.status.as_str()))
                .unwrap_or("-");

            Row::new(vec![
                Cell::from(runner.id.to_string()),
                Cell::from(runner.tag_list.join(", ")),
                Cell::from(runner.managers.len().to_string()),
                Cell::from(
                    oldest
                        .map(|manager| manager.system_id.as_str())
                        .unwrap_or("-"),
                ),
                Cell::from(
                    oldest
                        .and_then(|manager| manager.version.as_deref())
                        .unwrap_or("-"),
                ),
                Cell::from(
                    newest
                        .map(|manager| manager.system_id.as_str())
                        .unwrap_or("-"),
                ),
                Cell::from(
                    newest
                        .and_then(|manager| manager.version.as_deref())
                        .unwrap_or("-"),
                ),
                Cell::from(overall_status).style(styles::status_style(overall_status)),
            ])
        } else {
            Row::new(vec![
                Cell::from(runner.id.to_string()),
                Cell::from(runner.runner_type.as_str()),
                Cell::from(runner.status.as_str()).style(styles::status_style(&runner.status)),
                Cell::from(dash_or(&runner.version)),
                Cell::from(runner.tag_list.join(", ")),
                Cell::from(runner.managers.len().to_string()),
                Cell::from(dash_or(&runner.ip_address)),
            ])
        }
    });

    let widths = if rotating {
        vec![
            Constraint::Length(8),
            Constraint::Percentage(24),
            Constraint::Length(6),
            Constraint::Percentage(18),
            Constraint::Length(10),
            Constraint::Percentage(18),
            Constraint::Length(10),
            Constraint::Length(10),
        ]
    } else {
        vec![
            Constraint::Length(8),
            Constraint::Length(14),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Percentage(28),
            Constraint::Length(6),
            Constraint::Length(15),
        ]
    };

    let table = Table::new(rows, widths)
        .header(header)
        .highlight_style(styles::selected_row_style())
        .block(styles::block(app.current_tab_title()));

    frame.render_stateful_widget(table, area, &mut app.table_state);
}

fn render_workers_tab(app: &mut App, frame: &mut Frame, area: Rect) {
    if app.manager_rows.is_empty() {
        let paragraph = Paragraph::new(app.active_tab().empty_label())
            .style(styles::muted_style())
            .block(styles::block(app.current_tab_title()))
            .wrap(Wrap { trim: true });
        frame.render_widget(paragraph, area);
        return;
    }

    match detail_layout_mode(area.width, area.height) {
        DetailLayoutMode::SidePanel => {
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(68), Constraint::Percentage(32)])
                .split(area);
            render_workers_table(app, frame, chunks[0]);
            render_worker_detail(app, frame, chunks[1]);
        }
        DetailLayoutMode::BottomPanel => {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(1), Constraint::Length(10)])
                .split(area);
            render_workers_table(app, frame, chunks[0]);
            render_worker_detail(app, frame, chunks[1]);
        }
        DetailLayoutMode::Compact => {
            render_workers_table(app, frame, area);
        }
    }
}

fn render_workers_table(app: &mut App, frame: &mut Frame, area: Rect) {
    let header = Row::new(vec![
        Cell::from("Runner"),
        Cell::from("Tags"),
        Cell::from("Worker"),
        Cell::from("System"),
        Cell::from("Status"),
        Cell::from("Version"),
        Cell::from("Contacted"),
    ])
    .style(styles::table_header_style());

    let rows = app.manager_rows.iter().map(|row| {
        Row::new(vec![
            Cell::from(row.runner_id.to_string()),
            Cell::from(row.runner_tags.join(", ")),
            Cell::from(row.manager.id.to_string()),
            Cell::from(row.manager.system_id.as_str()),
            Cell::from(row.manager.status.as_str())
                .style(styles::status_style(&row.manager.status)),
            Cell::from(dash_or(&row.manager.version)),
            Cell::from(row.manager.contacted_at.as_deref().unwrap_or("Never")),
        ])
    });

    let table = Table::new(
        rows,
        [
            Constraint::Length(8),
            Constraint::Percentage(22),
            Constraint::Length(8),
            Constraint::Percentage(22),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Percentage(24),
        ],
    )
    .header(header)
    .highlight_style(styles::selected_row_style())
    .block(styles::block(app.current_tab_title()));

    frame.render_stateful_widget(table, area, &mut app.table_state);
}

fn render_runner_detail(app: &App, frame: &mut Frame, area: Rect) {
    let Some(runner) = app.selected_runner() else {
        let paragraph = Paragraph::new("Select a runner to inspect its details.")
            .style(styles::muted_style())
            .block(styles::block("Details"));
        frame.render_widget(paragraph, area);
        return;
    };

    let mut items = vec![
        ListItem::new(format!("ID: {}", runner.id)),
        ListItem::new(format!("Type: {}", runner.runner_type)),
        ListItem::new(format!("Status: {}", runner.status))
            .style(styles::status_style(&runner.status)),
        ListItem::new(format!(
            "Version: {}",
            runner.version.as_deref().unwrap_or("-")
        )),
        ListItem::new(format!(
            "Revision: {}",
            runner.revision.as_deref().unwrap_or("-")
        )),
        ListItem::new(format!("Tags: {}", runner.tag_list.join(", "))),
        ListItem::new(format!(
            "IP: {}",
            runner.ip_address.as_deref().unwrap_or("-")
        )),
        ListItem::new(format!("Managers: {}", runner.managers.len())),
    ];

    if let Some(description) = &runner.description {
        items.push(ListItem::new(format!("Description: {}", description)));
    }

    if !runner.managers.is_empty() {
        items.push(ListItem::new(" "));
        items.push(ListItem::new("Managers:").style(styles::accent_style()));
        for manager in &runner.managers {
            items.push(ListItem::new(format!(
                "{} [{}] {}",
                manager.system_id,
                manager.status,
                manager.version.as_deref().unwrap_or("-")
            )));
        }
    }

    let list = List::new(items).block(styles::block("Runner Detail"));
    frame.render_widget(list, area);
}

fn render_worker_detail(app: &App, frame: &mut Frame, area: Rect) {
    let Some(row) = app.selected_manager_row() else {
        let paragraph = Paragraph::new("Select a worker row to inspect manager details.")
            .style(styles::muted_style())
            .block(styles::block("Details"));
        frame.render_widget(paragraph, area);
        return;
    };

    let items = vec![
        ListItem::new(format!("Runner ID: {}", row.runner_id)),
        ListItem::new(format!("Runner Tags: {}", row.runner_tags.join(", "))),
        ListItem::new(format!("Worker ID: {}", row.manager.id)),
        ListItem::new(format!("System ID: {}", row.manager.system_id)),
        ListItem::new(format!("Status: {}", row.manager.status))
            .style(styles::status_style(&row.manager.status)),
        ListItem::new(format!(
            "Contacted: {}",
            row.manager.contacted_at.as_deref().unwrap_or("Never")
        )),
        ListItem::new(format!(
            "IP: {}",
            row.manager.ip_address.as_deref().unwrap_or("-")
        )),
        ListItem::new(format!(
            "Version: {}",
            row.manager.version.as_deref().unwrap_or("-")
        )),
        ListItem::new(format!(
            "Revision: {}",
            row.manager.revision.as_deref().unwrap_or("-")
        )),
    ];

    let list = List::new(items).block(styles::block("Worker Detail"));
    frame.render_widget(list, area);
}

fn render_status_bar(app: &App, frame: &mut Frame, area: Rect) {
    let mut status = match app.mode {
        AppMode::Help => "Any key closes help".to_string(),
        AppMode::FilterInput => "Type tags | Enter: fetch active tab | Esc: stop editing".to_string(),
        AppMode::Dashboard => {
            "Tab/Shift+Tab: switch views | 1-7: jump | Enter/r: refresh | p: poll | ?: help | q: quit"
                .to_string()
        }
    };

    if matches!(
        detail_layout_mode(area.width, 24),
        DetailLayoutMode::Compact
    ) {
        if let Some(summary) = app.compact_selection_summary() {
            status.push_str(" | ");
            status.push_str(&summary);
        }
    }

    let paragraph = Paragraph::new(status).block(styles::block("Status"));
    frame.render_widget(paragraph, area);
}

fn render_help_view(frame: &mut Frame, area: Rect) {
    let help = vec![
        Line::from("GitLab Runner TUI"),
        Line::from(""),
        Line::from("Navigation"),
        Line::from("  Tab / Shift+Tab  Switch top-level views"),
        Line::from("  1-7              Jump directly to a view"),
        Line::from("  ↑/↓ or j/k       Move table selection"),
        Line::from(""),
        Line::from("Actions"),
        Line::from("  Enter            Fetch the active tab"),
        Line::from("  r                Refresh the active tab"),
        Line::from("  p                Toggle polling / auto-refresh"),
        Line::from("  q                Quit"),
        Line::from(""),
        Line::from("Filtering"),
        Line::from("  Type any tag text to focus the filter bar"),
        Line::from("  Enter            Apply filter to the active tab"),
        Line::from("  Esc              Exit filter editing"),
        Line::from(""),
        Line::from("Views"),
        Line::from("  1 Runners   2 Health   3 Offline   4 Uncontacted"),
        Line::from("  5 Empty     6 Rotating 7 Workers"),
    ];

    let paragraph = Paragraph::new(help)
        .block(styles::block("Help"))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}
