use crate::config::RunnerDiscoveryMode;
use crate::models::runner::{RunnerSortKey, TagFilterMode};
use crate::tui::{
    app::{
        detail_layout_mode, latest_runner_contact_label, manager_contact_detail,
        manager_contact_label, App, AppMode, DetailLayoutMode, FilterPopupSection,
        PollDisplayState, ResultsViewType, SettingsField, Tab,
    },
    styles,
};
use chrono::Utc;
use ratatui::{
    layout::{Alignment, Constraint, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Cell, Clear, HighlightSpacing, List, ListItem, Paragraph, Row, Scrollbar,
        ScrollbarOrientation, ScrollbarState, Table, Wrap,
    },
    Frame,
};

// ⚡ Bolt: Return `&str` instead of `String`
// This avoids calling `.to_string()` and creating heap allocations
// for every runner row rendered in the TUI table.
fn dash_or(value: &Option<String>) -> &str {
    value.as_deref().unwrap_or("-")
}

fn display_or_dash(value: &str) -> &str {
    if value.trim().is_empty() {
        "-"
    } else {
        value.trim()
    }
}

pub fn render(app: &mut App, frame: &mut Frame) {
    let chunks = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(3),
    ])
    .split(frame.area());

    render_header(app, frame, chunks[0]);
    render_tabs(app, frame, chunks[1]);

    match app.mode {
        AppMode::Help => render_help_view(frame, chunks[2]),
        _ => render_content(app, frame, chunks[2]),
    }

    render_status_bar(app, frame, chunks[3]);

    match app.mode {
        AppMode::Settings => render_settings_modal(app, frame),
        AppMode::FilterPopup => render_filter_popup(app, frame),
        _ => {}
    }
}

fn centered_rect(width_percent: u16, height_percent: u16, area: Rect) -> Rect {
    let vert = Layout::vertical([
        Constraint::Percentage((100 - height_percent) / 2),
        Constraint::Percentage(height_percent),
        Constraint::Percentage((100 - height_percent) / 2),
    ])
    .split(area);

    Layout::horizontal([
        Constraint::Percentage((100 - width_percent) / 2),
        Constraint::Percentage(width_percent),
        Constraint::Percentage((100 - width_percent) / 2),
    ])
    .split(vert[1])[1]
}

fn render_header(app: &App, frame: &mut Frame, area: Rect) {
    let chunks = Layout::horizontal([Constraint::Min(24), Constraint::Length(32)]).split(area);

    let title = if app.is_loading {
        format!(
            "GitLab Runner TUI {} {}",
            app.spinner_char(),
            app.active_tab().loading_label()
        )
    } else {
        "GitLab Runner TUI".to_string()
    };

    let title_line = styles::gradient_text(&title, (125, 207, 255), (187, 154, 247));

    let header = Paragraph::new(vec![title_line]).block(styles::block("Dashboard"));
    frame.render_widget(header, chunks[0]);
    render_polling_widget(app, frame, chunks[1]);
}

fn render_polling_widget(app: &App, frame: &mut Frame, area: Rect) {
    let block = styles::block("Polling [p]");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width < 10 || inner.height < 1 {
        return;
    }

    let state = app.poll_display_state();
    let badge = match state {
        PollDisplayState::Refreshing => format!("{} Refresh", app.spinner_char()),
        PollDisplayState::Live => "● Live".to_string(),
        PollDisplayState::Paused => "Paused".to_string(),
        PollDisplayState::TimedOut => "Timed out".to_string(),
        PollDisplayState::Error => "Error".to_string(),
    };

    let age = app
        .last_refresh_age_secs()
        .map(format_age)
        .unwrap_or_else(|| "never".to_string());
    let next = app
        .next_poll_in_secs()
        .map(format_age)
        .unwrap_or_else(|| "-".to_string());

    let status_style = match state {
        PollDisplayState::Live | PollDisplayState::Refreshing => styles::accent_style(),
        PollDisplayState::Paused => styles::muted_style(),
        PollDisplayState::TimedOut => styles::status_style("stale"),
        PollDisplayState::Error => styles::status_style("offline"),
    };

    let mut line = if matches!(state, PollDisplayState::Live | PollDisplayState::Refreshing) {
        styles::gradient_text(&badge, (125, 207, 255), (187, 154, 247))
    } else {
        Line::from(Span::styled(&badge, status_style))
    };
    line.spans.push(Span::styled(" · ", styles::muted_style()));
    line.spans.push(Span::styled(
        format!("Prev {age} Next {next}"),
        styles::muted_style(),
    ));

    frame.render_widget(Paragraph::new(line), inner);
}

fn format_age(seconds: u64) -> String {
    match seconds {
        0..=89 => format!("{}s", seconds),
        90..=3599 => format!("{}m", seconds / 60),
        3600..=86_399 => format!("{}h", seconds / 3600),
        _ => format!("{}d", seconds / 86_400),
    }
}

fn tab_chip_colors(tab: Tab) -> (Color, Color) {
    match tab {
        Tab::Runners => (Color::Rgb(125, 207, 255), Color::Rgb(30, 42, 66)),
        Tab::Offline => (Color::Rgb(247, 118, 142), Color::Rgb(58, 26, 30)),
        Tab::Uncontacted => (Color::Rgb(224, 175, 104), Color::Rgb(58, 42, 10)),
        Tab::Empty => (Color::Rgb(86, 95, 137), Color::Rgb(34, 37, 58)),
        Tab::Rotating => (Color::Rgb(187, 154, 247), Color::Rgb(42, 26, 62)),
        Tab::Health | Tab::Workers => (Color::Rgb(125, 207, 255), Color::Rgb(30, 42, 66)),
    }
}

fn push_separator(spans: &mut Vec<Span<'static>>) {
    if !spans.is_empty() {
        spans.push(Span::styled(" · ", styles::muted_style()));
    }
}

fn push_hotkey_hint(spans: &mut Vec<Span<'static>>, key: &str, label: &str, emphasized: bool) {
    push_separator(spans);
    spans.push(if emphasized {
        styles::hotkey_chip(key)
    } else {
        styles::muted_chip(key)
    });
    spans.push(Span::raw(" "));
    spans.push(Span::styled(label.to_string(), styles::muted_style()));
}

fn build_dashboard_shortcuts_line(compact: bool, active_tab: Tab) -> Line<'static> {
    let mut spans = Vec::new();
    if compact {
        push_hotkey_hint(&mut spans, "r", "refresh", true);
        push_hotkey_hint(&mut spans, "f", "filter", true);
        if active_tab == Tab::Uncontacted {
            push_hotkey_hint(&mut spans, "a", "cutoff", true);
        }
        push_hotkey_hint(&mut spans, "q", "quit", true);
    } else {
        push_hotkey_hint(&mut spans, "p", "poll", true);
        push_hotkey_hint(&mut spans, "r", "refresh", true);
        push_hotkey_hint(&mut spans, "f", "filter", true);
        if active_tab == Tab::Uncontacted {
            push_hotkey_hint(&mut spans, "a", "cutoff", true);
        }
        push_hotkey_hint(&mut spans, "s", "sort", false);
        push_hotkey_hint(&mut spans, "c", "settings", false);
        push_hotkey_hint(&mut spans, "?", "help", false);
        push_hotkey_hint(&mut spans, "q", "quit", true);
    }
    Line::from(spans)
}

fn simple_status_hint(items: &[(&str, &str)]) -> Line<'static> {
    let mut spans = Vec::new();
    for (key, label) in items {
        push_hotkey_hint(&mut spans, key, label, true);
    }
    Line::from(spans)
}

fn render_tabs(app: &App, frame: &mut Frame, area: Rect) {
    let mut spans: Vec<Span> = Vec::new();
    spans.push(Span::raw(" "));

    for (i, tab) in app.tabs.iter().enumerate() {
        let is_active = i == app.active_tab_index;

        if i > 0 {
            spans.push(Span::styled("  ", styles::muted_style()));
        }

        spans.push(if is_active {
            styles::hotkey_chip(tab.shortcut().to_string())
        } else {
            styles::muted_chip(tab.shortcut().to_string())
        });
        spans.push(Span::raw(" "));

        let name_style = if is_active {
            Style::default()
                .fg(Color::White)
                .bg(tab_chip_colors(*tab).1)
                .add_modifier(Modifier::BOLD)
        } else {
            styles::muted_style()
        };
        spans.push(Span::styled(format!(" {} ", tab.title()), name_style));

        if let Some(&count) = app.tab_counts.get(tab) {
            let (chip_fg, chip_bg) = tab_chip_colors(*tab);
            spans.push(Span::raw(" "));
            spans.push(styles::soft_badge(
                count.to_string(),
                chip_fg,
                chip_bg,
                is_active,
            ));
        }
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render_content(app: &mut App, frame: &mut Frame, area: Rect) {
    if let Some(error) = &app.error_message {
        render_error(error, frame, area);
        return;
    }

    if !app.has_loaded_active_tab() {
        let prompt = Paragraph::new(format!(
            "Loading the {} tab will use the current tag filter. Press r to retry if needed.",
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
        Line::from(
            "Check GITLAB_HOST, GITLAB_TOKEN, runner target config, and network connectivity.",
        ),
        Line::from("Press Esc to dismiss the error."),
    ];

    let paragraph = Paragraph::new(lines)
        .block(styles::error_block("Error"))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn render_health_tab(app: &mut App, frame: &mut Frame, area: Rect) {
    let chunks = Layout::vertical([Constraint::Length(5), Constraint::Min(1)]).split(area);

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

    if app.active_tab() == Tab::Empty {
        render_runner_table(app, frame, area, false);
        return;
    }

    match detail_layout_mode(area.width, area.height) {
        DetailLayoutMode::SidePanel => {
            let chunks =
                Layout::horizontal([Constraint::Percentage(66), Constraint::Percentage(34)])
                    .split(area);
            render_runner_table(app, frame, chunks[0], rotating);
            render_runner_detail(app, frame, chunks[1]);
        }
        DetailLayoutMode::BottomPanel => {
            let chunks = Layout::vertical([Constraint::Min(1), Constraint::Length(10)]).split(area);
            render_runner_table(app, frame, chunks[0], rotating);
            render_runner_detail(app, frame, chunks[1]);
        }
        DetailLayoutMode::Compact => {
            render_runner_table(app, frame, area, rotating);
        }
    }
}

fn header_with_indicator(
    label: &str,
    target: RunnerSortKey,
    active: RunnerSortKey,
) -> Cell<'static> {
    if active == target {
        let symbol = match active {
            RunnerSortKey::LastContact => "↑",
            RunnerSortKey::None => "",
            _ => "↓",
        };
        Cell::from(format!("{label} {symbol}"))
    } else {
        Cell::from(label.to_string())
    }
}

fn sort_column_index(rotating: bool, tab: Tab, sort_key: RunnerSortKey) -> Option<usize> {
    if sort_key == RunnerSortKey::None {
        return None;
    }
    if rotating {
        return match sort_key {
            RunnerSortKey::Status => Some(3),
            RunnerSortKey::LastContact => Some(4),
            _ => None,
        };
    }
    match tab {
        Tab::Runners => match sort_key {
            RunnerSortKey::Status => Some(1),
            RunnerSortKey::Version => Some(2),
            RunnerSortKey::LastContact => Some(3),
            RunnerSortKey::Tags => Some(4),
            RunnerSortKey::Managers => Some(5),
            _ => None,
        },
        Tab::Health => match sort_key {
            RunnerSortKey::Status => Some(1),
            RunnerSortKey::Version => Some(2),
            RunnerSortKey::LastContact => Some(3),
            RunnerSortKey::Managers => Some(4),
            _ => None,
        },
        Tab::Offline | Tab::Uncontacted => match sort_key {
            RunnerSortKey::Version => Some(1),
            RunnerSortKey::LastContact => Some(2),
            RunnerSortKey::Managers => Some(3),
            _ => None,
        },
        Tab::Empty => match sort_key {
            RunnerSortKey::Version => Some(1),
            RunnerSortKey::Status => Some(2),
            _ => None,
        },
        Tab::Workers => match sort_key {
            RunnerSortKey::Status => Some(3),
            RunnerSortKey::LastContact => Some(4),
            _ => None,
        },
        _ => None,
    }
}

fn render_runner_table(app: &mut App, frame: &mut Frame, area: Rect, rotating: bool) {
    let now = Utc::now();
    let active_tab = app.active_tab();
    let active_sort = app.effective_sort_key();
    let header = if rotating {
        Row::new(vec![
            Cell::from("ID"),
            Cell::from("Old System"),
            Cell::from("New System"),
            header_with_indicator("Status", RunnerSortKey::Status, active_sort),
            header_with_indicator("Last Contact", RunnerSortKey::LastContact, active_sort),
        ])
    } else {
        match active_tab {
            Tab::Runners => Row::new(vec![
                Cell::from("ID"),
                header_with_indicator("Status", RunnerSortKey::Status, active_sort),
                header_with_indicator("Version", RunnerSortKey::Version, active_sort),
                header_with_indicator("Last Contact", RunnerSortKey::LastContact, active_sort),
                header_with_indicator("Tags", RunnerSortKey::Tags, active_sort),
                header_with_indicator("Mgrs", RunnerSortKey::Managers, active_sort),
            ]),
            Tab::Health => Row::new(vec![
                Cell::from("ID"),
                header_with_indicator("Status", RunnerSortKey::Status, active_sort),
                header_with_indicator("Version", RunnerSortKey::Version, active_sort),
                header_with_indicator("Last Contact", RunnerSortKey::LastContact, active_sort),
                header_with_indicator("Mgrs", RunnerSortKey::Managers, active_sort),
            ]),
            Tab::Offline | Tab::Uncontacted => Row::new(vec![
                Cell::from("ID"),
                header_with_indicator("Version", RunnerSortKey::Version, active_sort),
                header_with_indicator("Last Contact", RunnerSortKey::LastContact, active_sort),
                header_with_indicator("Mgrs", RunnerSortKey::Managers, active_sort),
            ]),
            Tab::Empty => Row::new(vec![
                Cell::from("ID"),
                header_with_indicator("Version", RunnerSortKey::Version, active_sort),
                header_with_indicator("Status", RunnerSortKey::Status, active_sort),
            ]),
            _ => Row::new(vec![
                Cell::from("ID"),
                header_with_indicator("Status", RunnerSortKey::Status, active_sort),
                header_with_indicator("Version", RunnerSortKey::Version, active_sort),
                header_with_indicator("Last Contact", RunnerSortKey::LastContact, active_sort),
                header_with_indicator("Tags", RunnerSortKey::Tags, active_sort),
                header_with_indicator("Mgrs", RunnerSortKey::Managers, active_sort),
            ]),
        }
    }
    .style(styles::table_header_style());

    let rows = app.runners.iter().map(|uirunner| {
        let runner = &uirunner.runner;
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
                Cell::from(runner.id.to_string()).style(styles::muted_style()),
                Cell::from(
                    oldest
                        .map(|manager| manager.system_id.as_str())
                        .unwrap_or("-"),
                ),
                Cell::from(
                    newest
                        .map(|manager| manager.system_id.as_str())
                        .unwrap_or("-"),
                ),
                Cell::from(styles::status_line(overall_status)),
                Cell::from(latest_runner_contact_label(runner, now)),
            ])
        } else {
            let status = Cell::from(styles::status_line(runner.status.as_str()));
            let last_contact = Cell::from(latest_runner_contact_label(runner, now));

            match active_tab {
                Tab::Runners => Row::new(vec![
                    Cell::from(runner.id.to_string()).style(styles::muted_style()),
                    status,
                    Cell::from(dash_or(&runner.version)),
                    last_contact,
                    Cell::from(uirunner.formatted_tags.as_str()),
                    Cell::from(runner.managers.len().to_string()).style(styles::muted_style()),
                ]),
                Tab::Health => Row::new(vec![
                    Cell::from(runner.id.to_string()).style(styles::muted_style()),
                    status,
                    Cell::from(dash_or(&runner.version)),
                    last_contact,
                    Cell::from(runner.managers.len().to_string()).style(styles::muted_style()),
                ]),
                Tab::Offline | Tab::Uncontacted => Row::new(vec![
                    Cell::from(runner.id.to_string()).style(styles::muted_style()),
                    Cell::from(dash_or(&runner.version)),
                    last_contact,
                    Cell::from(runner.managers.len().to_string()).style(styles::muted_style()),
                ]),
                Tab::Empty => Row::new(vec![
                    Cell::from(runner.id.to_string()).style(styles::muted_style()),
                    Cell::from(dash_or(&runner.version)),
                    status,
                ]),
                _ => Row::new(vec![
                    Cell::from(runner.id.to_string()).style(styles::muted_style()),
                    status,
                    Cell::from(dash_or(&runner.version)),
                    last_contact,
                    Cell::from(uirunner.formatted_tags.as_str()),
                    Cell::from(runner.managers.len().to_string()).style(styles::muted_style()),
                ]),
            }
        }
    });

    let widths = if rotating {
        vec![
            Constraint::Length(8),
            Constraint::Percentage(30),
            Constraint::Percentage(30),
            Constraint::Length(10),
            Constraint::Length(14),
        ]
    } else {
        match active_tab {
            Tab::Runners => vec![
                Constraint::Length(8),
                Constraint::Length(10),
                Constraint::Length(10),
                Constraint::Length(14),
                Constraint::Percentage(40),
                Constraint::Length(6),
            ],
            Tab::Health => vec![
                Constraint::Length(8),
                Constraint::Length(10),
                Constraint::Length(10),
                Constraint::Length(14),
                Constraint::Length(6),
            ],
            Tab::Offline | Tab::Uncontacted => vec![
                Constraint::Length(8),
                Constraint::Length(10),
                Constraint::Length(14),
                Constraint::Length(6),
            ],
            Tab::Empty => vec![
                Constraint::Length(8),
                Constraint::Length(10),
                Constraint::Length(10),
            ],
            _ => vec![
                Constraint::Length(8),
                Constraint::Length(10),
                Constraint::Length(10),
                Constraint::Length(14),
                Constraint::Percentage(40),
                Constraint::Length(6),
            ],
        }
    };

    let col_idx = sort_column_index(rotating, active_tab, active_sort);
    app.table_state.select_column(col_idx);

    let table = Table::new(rows, widths)
        .header(header)
        .row_highlight_style(styles::selected_row_style())
        .column_highlight_style(styles::sort_column_style())
        .highlight_symbol("▶ ")
        .highlight_spacing(HighlightSpacing::Always)
        .block(styles::block(app.current_tab_title()));

    frame.render_stateful_widget(table, area, &mut app.table_state);
    render_table_scrollbar(app, frame, area);
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
            let chunks =
                Layout::horizontal([Constraint::Percentage(68), Constraint::Percentage(32)])
                    .split(area);
            render_workers_table(app, frame, chunks[0]);
            render_worker_detail(app, frame, chunks[1]);
        }
        DetailLayoutMode::BottomPanel => {
            let chunks = Layout::vertical([Constraint::Min(1), Constraint::Length(10)]).split(area);
            render_workers_table(app, frame, chunks[0]);
            render_worker_detail(app, frame, chunks[1]);
        }
        DetailLayoutMode::Compact => {
            render_workers_table(app, frame, area);
        }
    }
}

fn render_workers_table(app: &mut App, frame: &mut Frame, area: Rect) {
    let now = Utc::now();
    let active_sort = app.effective_sort_key();
    let header = Row::new(vec![
        Cell::from("Runner"),
        Cell::from("Worker"),
        Cell::from("System"),
        header_with_indicator("Status", RunnerSortKey::Status, active_sort),
        header_with_indicator("Contacted", RunnerSortKey::LastContact, active_sort),
    ])
    .style(styles::table_header_style());

    let rows = app.manager_rows.iter().map(|row| {
        Row::new(vec![
            Cell::from(row.runner_id.to_string()).style(styles::muted_style()),
            Cell::from(row.manager.id.to_string()).style(styles::muted_style()),
            Cell::from(row.manager.system_id.as_str()),
            Cell::from(styles::status_line(row.manager.status.as_str())),
            Cell::from(manager_contact_label(&row.manager, now)),
        ])
    });

    let table = Table::new(
        rows,
        [
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Percentage(34),
            Constraint::Length(10),
            Constraint::Length(14),
        ],
    );

    let col_idx = sort_column_index(false, Tab::Workers, app.effective_sort_key());
    app.table_state.select_column(col_idx);

    let table = table
        .header(header)
        .row_highlight_style(styles::selected_row_style())
        .column_highlight_style(styles::sort_column_style())
        .highlight_symbol("▶ ")
        .highlight_spacing(HighlightSpacing::Always)
        .block(styles::block(app.current_tab_title()));

    frame.render_stateful_widget(table, area, &mut app.table_state);
    render_table_scrollbar(app, frame, area);
}

fn render_table_scrollbar(app: &mut App, frame: &mut Frame, area: Rect) {
    if app.active_result_len() <= 1 {
        return;
    }
    let viewport = area.height.saturating_sub(3) as usize;
    app.scroll_state = ScrollbarState::new(app.active_result_len())
        .position(app.table_state.offset())
        .viewport_content_length(viewport.max(1));

    frame.render_stateful_widget(
        Scrollbar::default()
            .orientation(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None),
        area.inner(Margin {
            vertical: 1,
            horizontal: 0,
        }),
        &mut app.scroll_state,
    );
}

fn render_runner_detail(app: &App, frame: &mut Frame, area: Rect) {
    let Some(ui_runner) = app.selected_ui_runner() else {
        let paragraph = Paragraph::new("Select a runner to inspect its details.")
            .style(styles::muted_style())
            .block(styles::block("Details"));
        frame.render_widget(paragraph, area);
        return;
    };
    let runner = &ui_runner.runner;

    let now = Utc::now();

    // Compute wrappable tags lines given available inner width (subtract 2 for border).
    let inner_width = area.width.saturating_sub(2) as usize;
    let tag_items: Vec<ListItem> = if runner.tag_list.is_empty() {
        vec![ListItem::new("Tags: -")]
    } else {
        let prefix = "Tags: ";
        let prefix_len = prefix.len();
        let mut lines: Vec<ratatui::text::Line> = Vec::new();
        let mut current_line = prefix.to_string();
        for (i, tag) in runner.tag_list.iter().enumerate() {
            let sep = if i == 0 { "" } else { ", " };
            let added_len = sep.len() + tag.len();
            if i > 0 && current_line.len() + added_len > inner_width {
                lines.push(ratatui::text::Line::from(std::mem::take(&mut current_line)));
                current_line.push_str(&" ".repeat(prefix_len));
                current_line.push_str(tag);
            } else {
                current_line.push_str(sep);
                current_line.push_str(tag);
            }
        }
        lines.push(ratatui::text::Line::from(current_line));
        vec![ListItem::new(ratatui::text::Text::from(lines))]
    };

    let display_type = match runner.runner_type.as_str() {
        "project_type" => "project",
        "group_type" => "group",
        "instance_type" => "instance",
        other => other,
    };

    let registered = runner
        .created_at
        .as_deref()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| {
            let age = (now - dt.with_timezone(&Utc)).num_seconds().max(0) as u64;
            format!("{} ago", format_age(age))
        })
        .unwrap_or_else(|| "-".to_string());

    let mut items = vec![ListItem::new(format!("ID: {}", runner.id))];

    if let Some(desc) = &runner.description {
        if !desc.trim().is_empty() {
            items.push(ListItem::new(format!("Name: {desc}")));
        }
    }

    if runner.paused {
        items.push(
            ListItem::new("⚠  Paused — will not pick up jobs").style(styles::status_style("stale")),
        );
    }

    items.push(ListItem::new(format!("Type: {display_type}")));

    items.push(ListItem::new(format!(
        "Last Contact: {}",
        latest_runner_contact_label(runner, now)
    )));
    items.push(ListItem::new(format!("Registered: {registered}")));
    items.push(ListItem::new(format!(
        "Version: {}",
        runner.version.as_deref().unwrap_or("-")
    )));
    items.push(ListItem::new(format!(
        "Revision: {}",
        runner.revision.as_deref().unwrap_or("-")
    )));

    if let Some(ip) = &runner.ip_address {
        items.push(ListItem::new(format!("IP: {ip}")));
    }

    if let Some(formatted_groups) = &ui_runner.formatted_groups {
        items.push(ListItem::new(format!("Groups: {formatted_groups}")));
    }

    items.extend(tag_items);

    if !runner.managers.is_empty() {
        items.push(ListItem::new(" "));
        items.push(ListItem::new("Managers:").style(styles::accent_style()));
        for manager in &runner.managers {
            let platform = match (manager.platform.as_deref(), manager.architecture.as_deref()) {
                (Some(p), Some(a)) => format!("  {p}/{a}"),
                (Some(p), None) => format!("  {p}"),
                _ => String::new(),
            };
            let ip = manager
                .ip_address
                .as_deref()
                .map(|ip| format!("  {ip}"))
                .unwrap_or_default();
            items.push(ListItem::new(format!(
                "{} [{}] {} {}{}{}",
                manager.system_id,
                manager.status,
                manager_contact_label(manager, now),
                manager.version.as_deref().unwrap_or("-"),
                platform,
                ip,
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
        ListItem::new(format!("Worker ID: {}", row.manager.id)),
        ListItem::new(format!("System ID: {}", row.manager.system_id)),
        ListItem::new(format!(
            "Contacted: {}",
            manager_contact_detail(&row.manager)
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

fn render_filter_popup(app: &mut App, frame: &mut Frame) {
    let area = centered_rect(60, 78, frame.area());
    frame.render_widget(Clear, area);

    let sections = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(5),
        Constraint::Length(5),
        Constraint::Min(6),
        Constraint::Length(7),
        Constraint::Length(1),
    ])
    .split(area);

    // --- Tag search section ---
    let search_focused = app.filter_popup_section == FilterPopupSection::TagSearch;
    let search_title = if search_focused {
        "▶ Search Tags"
    } else {
        "  Search Tags"
    };
    let search_text = if search_focused {
        format!("{}█", app.tag_search_input)
    } else if app.tag_search_input.is_empty() {
        "Type to filter tags...".to_string()
    } else {
        app.tag_search_input.clone()
    };
    let search_style = if search_focused {
        styles::selected_row_style()
    } else if app.tag_search_input.is_empty() {
        styles::muted_style()
    } else {
        ratatui::style::Style::default()
    };
    let search_widget = ratatui::widgets::Paragraph::new(search_text)
        .style(search_style)
        .block(if search_focused {
            styles::focused_block(search_title)
        } else {
            styles::block(search_title)
        });
    frame.render_widget(search_widget, sections[0]);

    // --- Tags section ---
    let selected_focused = app.filter_popup_section == FilterPopupSection::Selected;
    let selected_title = if selected_focused {
        "▶ Selected Filters"
    } else {
        "  Selected Filters"
    };
    let selected_items_raw = app.selected_filter_items();
    let selected_items: Vec<ListItem> = if selected_items_raw.is_empty() {
        vec![ListItem::new("No filters selected.")]
    } else {
        selected_items_raw
            .iter()
            .map(|item| match item {
                crate::tui::app::SelectedFilterItem::TextTag(tag) => {
                    ListItem::new(format!("[x] text:{tag}"))
                }
                crate::tui::app::SelectedFilterItem::PopupTag(tag) => {
                    ListItem::new(format!("[x] tag:{tag}"))
                }
                crate::tui::app::SelectedFilterItem::Version(version) => {
                    ListItem::new(format!("[x] version:{version}"))
                }
                crate::tui::app::SelectedFilterItem::Status(status) => {
                    ListItem::new(format!("[x] status:{status}"))
                }
            })
            .collect()
    };
    let selected_list = List::new(selected_items)
        .highlight_style(if selected_focused {
            styles::selected_row_style()
        } else {
            styles::muted_style()
        })
        .block(if selected_focused {
            styles::focused_block(selected_title)
        } else {
            styles::block(selected_title)
        });
    if selected_focused {
        frame.render_stateful_widget(
            selected_list,
            sections[1],
            &mut app.selected_filter_list_state,
        );
    } else {
        frame.render_widget(selected_list, sections[1]);
    }

    let status_focused = app.filter_popup_section == FilterPopupSection::Status;
    let status_title = if status_focused {
        "▶ Status"
    } else {
        "  Status"
    };
    let status_items: Vec<ListItem> = app
        .status_options
        .iter()
        .map(|status| {
            let marker = if app.selected_status.as_deref() == Some(status.as_str()) {
                "[x]"
            } else {
                "[ ]"
            };
            ListItem::new(format!("{marker} {status}"))
        })
        .collect();
    let status_list = List::new(status_items)
        .highlight_style(if status_focused {
            styles::selected_row_style()
        } else {
            styles::muted_style()
        })
        .block(if status_focused {
            styles::focused_block(status_title)
        } else {
            styles::block(status_title)
        });
    if status_focused {
        frame.render_stateful_widget(status_list, sections[2], &mut app.status_list_state);
    } else {
        frame.render_widget(status_list, sections[2]);
    }

    // --- Tags section ---
    let tags_focused = app.filter_popup_section == FilterPopupSection::Tags;
    let mode_label = match app.tag_filter_mode {
        TagFilterMode::And => "[m: AND]",
        TagFilterMode::Or => "[m: OR] ",
    };
    let tags_title = if tags_focused {
        format!("▶ Tags {mode_label}")
    } else {
        format!("  Tags {mode_label}")
    };
    let tags_title = tags_title.as_str();
    let filtered_tags: Vec<&String> = app.filtered_tag_options().collect();
    let tag_items: Vec<ListItem> = if filtered_tags.is_empty() {
        vec![ListItem::new(if app.tag_options.is_empty() {
            "No tags discovered yet."
        } else {
            "No tags match the search."
        })]
    } else {
        filtered_tags
            .iter()
            .map(|tag| {
                let marker = if app.selected_tags.contains(tag) {
                    "[x]"
                } else {
                    "[ ]"
                };
                ListItem::new(format!("{marker} {tag}"))
            })
            .collect()
    };
    let tags_list = List::new(tag_items)
        .highlight_style(if tags_focused {
            styles::selected_row_style()
        } else {
            styles::muted_style()
        })
        .block(if tags_focused {
            styles::focused_block(tags_title)
        } else {
            styles::block(tags_title)
        });
    if tags_focused {
        frame.render_stateful_widget(tags_list, sections[3], &mut app.tag_list_state);
    } else {
        frame.render_widget(tags_list, sections[3]);
    }

    // --- Versions section ---
    let versions_focused = app.filter_popup_section == FilterPopupSection::Versions;
    let versions_title = if versions_focused {
        "▶ Versions"
    } else {
        "  Versions"
    };
    let version_items: Vec<ListItem> = if app.version_options.is_empty() {
        vec![ListItem::new("No versions loaded yet.")]
    } else {
        app.version_options
            .iter()
            .map(|version| {
                let marker = if app.selected_versions.contains(version) {
                    "[x]"
                } else {
                    "[ ]"
                };
                ListItem::new(format!("{marker} {version}"))
            })
            .collect()
    };
    let versions_list = List::new(version_items)
        .highlight_style(if versions_focused {
            styles::selected_row_style()
        } else {
            styles::muted_style()
        })
        .block(if versions_focused {
            styles::focused_block(versions_title)
        } else {
            styles::block(versions_title)
        });
    if versions_focused {
        frame.render_stateful_widget(versions_list, sections[4], &mut app.version_list_state);
    } else {
        frame.render_widget(versions_list, sections[4]);
    }

    // --- Footer ---
    let footer = Paragraph::new(
        "type:search  space:toggle/remove  m:AND/OR  c:clear section  a:clear all  tab:switch  esc:close",
    )
    .style(styles::muted_style());
    frame.render_widget(footer, sections[5]);
}

fn render_settings_modal(app: &mut App, frame: &mut Frame) {
    let area = centered_rect(72, 70, frame.area());
    frame.render_widget(Clear, area);

    let sections = Layout::vertical([
        Constraint::Length(10),
        Constraint::Length(8),
        Constraint::Min(8),
        Constraint::Length(3),
    ])
    .split(area);

    render_connection_section(app, frame, sections[0]);
    render_behavior_section(app, frame, sections[1]);
    render_diagnostics_section(app, frame, sections[2]);
    render_settings_footer(app, frame, sections[3]);
}

fn render_connection_section(app: &App, frame: &mut Frame, area: Rect) {
    let all_runners_label;
    let mode_label = match app.settings_draft.discovery_mode {
        RunnerDiscoveryMode::AllRunners => {
            if app.all_runners_fell_back {
                all_runners_label =
                    "All runners (/runners/all → fell back to /runners)".to_string();
                all_runners_label.as_str()
            } else {
                "All runners (/runners/all)"
            }
        }
        RunnerDiscoveryMode::VisibleRunners => "Visible runners (/runners)",
        RunnerDiscoveryMode::ConfiguredTargets => "Targets",
    };
    let is_token_selected = app.settings_draft.selected_field == SettingsField::Token;
    let token_display = if is_token_selected {
        if app.settings_draft.token.is_empty() {
            "-".to_string()
        } else {
            app.settings_draft.token.clone()
        }
    } else if app.settings_draft.token.is_empty() {
        "-".to_string()
    } else {
        "•".repeat(app.settings_draft.token.chars().count().min(24))
    };

    let lines = vec![
        field_line(
            app.settings_draft.selected_field == SettingsField::Host,
            "Host",
            &app.settings_draft.host,
        ),
        field_line(is_token_selected, "Token", &token_display),
        field_line(
            app.settings_draft.selected_field == SettingsField::DiscoveryMode,
            "Discovery",
            mode_label,
        ),
        field_line(
            app.settings_draft.selected_field == SettingsField::Targets,
            "Targets",
            display_or_dash(&app.settings_draft.runner_targets_input),
        ),
    ];

    let paragraph = Paragraph::new(lines)
        .block(styles::focused_block("Settings"))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn render_behavior_section(app: &App, frame: &mut Frame, area: Rect) {
    let lines = vec![
        field_line(
            app.settings_draft.selected_field == SettingsField::PollInterval,
            "Poll interval",
            &format!("{}s", app.settings_draft.poll_interval_input),
        ),
        field_line(
            app.settings_draft.selected_field == SettingsField::PollTimeout,
            "Poll timeout",
            &format!("{}s", app.settings_draft.poll_timeout_input),
        ),
        Line::from(format!(
            "Versions: {} | Sort: {}",
            app.selected_versions_summary(),
            app.sort_label()
        )),
    ];

    let paragraph = Paragraph::new(lines)
        .block(styles::block("Behavior"))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn render_diagnostics_section(app: &App, frame: &mut Frame, area: Rect) {
    let mut lines = Vec::new();

    if let Some(metrics) = &app.live_query_metrics {
        let mode = match metrics.discovery_mode {
            RunnerDiscoveryMode::AllRunners => {
                if app.all_runners_fell_back {
                    "/runners/all→/runners"
                } else {
                    "/runners/all"
                }
            }
            RunnerDiscoveryMode::VisibleRunners => "/runners",
            RunnerDiscoveryMode::ConfiguredTargets => "targets",
        };
        lines.push(Line::from(format!(
            "Live query: {} ms | {} results | mode {} | requests L/D/M {}/{}/{}",
            metrics.duration_millis,
            metrics.result_count,
            mode,
            metrics.request_counts.list_requests,
            metrics.request_counts.detail_requests,
            metrics.request_counts.manager_requests
        )));
        if !metrics.succeeded {
            lines.push(Line::from(format!(
                "Last failure: {}",
                metrics.error_message.as_deref().unwrap_or("unknown error")
            )));
        }
    } else {
        lines.push(Line::from("Live query: no measurements yet"));
    }

    lines.push(Line::from(" "));
    lines.push(Line::from("Local processing benchmarks [b]:"));

    if let Some(snapshot) = &app.local_benchmarks {
        if snapshot.measurements.is_empty() {
            lines.push(Line::from("No benchmark samples available."));
        } else {
            for measurement in &snapshot.measurements {
                let blazing = if measurement.filter_duration_micros <= 1_000 {
                    " blazing fast"
                } else {
                    ""
                };
                lines.push(Line::from(format!(
                    "{} rows -> filter {}us | sort {}us | flatten {}us | workers {}{}",
                    measurement.sample_size,
                    measurement.filter_duration_micros,
                    measurement.sort_duration_micros,
                    measurement.flatten_duration_micros,
                    measurement.worker_row_count,
                    blazing,
                )));
            }
        }
    } else {
        lines.push(Line::from(
            "Open settings or press b here to benchmark current data.",
        ));
    }

    if let Some(message) = &app.settings_message {
        lines.push(Line::from(" "));
        lines.push(Line::from(format!("Save error: {message}")));
    }

    let paragraph = Paragraph::new(lines)
        .block(styles::block("Diagnostics"))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn render_settings_footer(app: &App, frame: &mut Frame, area: Rect) {
    let save_style = if app.settings_draft.selected_field == SettingsField::Save {
        styles::accent_style()
    } else {
        styles::muted_style()
    };
    let cancel_style = if app.settings_draft.selected_field == SettingsField::Cancel {
        styles::accent_style()
    } else {
        styles::muted_style()
    };

    let footer = Paragraph::new(Line::from(vec![
        Span::styled("[Save]", save_style),
        Span::raw("  "),
        Span::styled("[Cancel]", cancel_style),
        Span::raw("  Enter: activate  Ctrl-S: save  Tab: next field  Esc: close"),
    ]))
    .alignment(Alignment::Center)
    .block(styles::block("Settings Actions"));
    frame.render_widget(footer, area);
}

fn field_line(selected: bool, label: &str, value: &str) -> Line<'static> {
    let style = if selected {
        styles::accent_style()
    } else {
        styles::muted_style()
    };
    Line::from(vec![
        Span::styled(format!("{label:<13}"), style),
        Span::raw(" "),
        Span::raw(value.to_string()),
    ])
}

fn render_status_bar(app: &App, frame: &mut Frame, area: Rect) {
    // For modal/overlay modes, render a simple centered hint.
    let simple_hint: Option<Line> = match app.mode {
        AppMode::Help => Some(simple_status_hint(&[("any", "close help")])),
        AppMode::FilterInput => Some(simple_status_hint(&[
            ("Enter", "apply"),
            ("Esc", "stop"),
            ("Ctrl-C", "quit"),
        ])),
        AppMode::StaleCutoffInput => Some(Line::from(vec![
            Span::styled("Cutoff ", styles::accent_style()),
            Span::raw(if app.stale_cutoff_input.is_empty() {
                "<default 1h>".to_string()
            } else {
                app.stale_cutoff_input.clone()
            }),
            Span::styled(" · ", styles::muted_style()),
            Span::styled("Enter", styles::accent_style()),
            Span::raw(" apply "),
            Span::styled("Esc", styles::accent_style()),
            Span::raw(" cancel"),
        ])),
        AppMode::FilterPopup => Some(simple_status_hint(&[
            ("f/esc", "close"),
            ("tab", "switch"),
            ("space", "toggle"),
            ("c", "clear"),
            ("a", "clear all"),
        ])),
        AppMode::Settings => Some(simple_status_hint(&[
            ("Tab", "move"),
            ("Type", "edit"),
            ("←/→", "discovery"),
            ("Ctrl-S", "save"),
            ("b", "bench"),
            ("Esc", "close"),
        ])),
        AppMode::Dashboard => None,
    };

    if let Some(hint) = simple_hint {
        let paragraph = Paragraph::new(hint).block(styles::block("Status"));
        frame.render_widget(paragraph, area);
        return;
    }

    let left_text = if app.is_loading {
        let filter = if app.filter_input.trim().is_empty() {
            "all runners".to_string()
        } else {
            format!("tags [{}]", app.filter_input.trim())
        };
        Line::from(vec![
            styles::status_chip(format!("{} LIVE", app.spinner_char()), "live"),
            Span::raw(" "),
            Span::styled(
                format!("Refreshing {} with {}", app.active_tab(), filter),
                styles::accent_style(),
            ),
        ])
    } else if let Some(error) = &app.error_message {
        Line::from(vec![
            styles::status_chip("ERROR", "error"),
            Span::raw(" "),
            Span::styled(
                format!(
                    "{}: {}",
                    app.active_tab(),
                    error.lines().next().unwrap_or("unknown error")
                ),
                styles::status_style("offline"),
            ),
        ])
    } else {
        let result_label = match app.current_results_view_type() {
            ResultsViewType::Workers => "workers",
            _ => "runners",
        };
        let refreshed = app
            .last_refresh_age_secs()
            .map(|age| format!("last refresh {} ago", format_age(age)))
            .unwrap_or_else(|| "not loaded yet".to_string());
        let mut left_summary = format!(
            "{} {} for {}",
            app.active_result_len(),
            result_label,
            app.active_tab(),
        );
        if matches!(
            detail_layout_mode(area.width, 24),
            DetailLayoutMode::Compact
        ) {
            if let Some(compact_summary) = app.compact_selection_summary() {
                left_summary.push_str(" · ");
                left_summary.push_str(&compact_summary);
            }
        }
        Line::from(vec![
            styles::status_chip("READY", "ok"),
            Span::raw(" "),
            Span::styled(left_summary, styles::accent_style()),
            Span::styled(" · ", styles::muted_style()),
            Span::styled(refreshed, styles::muted_style()),
        ])
    };

    let inner = area.inner(Margin {
        horizontal: 1,
        vertical: 1,
    });
    let compact_shortcuts = inner.width < 92;
    let shortcuts = build_dashboard_shortcuts_line(compact_shortcuts, app.active_tab());
    let shortcut_width = if compact_shortcuts {
        28
    } else if app.active_tab() == Tab::Uncontacted {
        76
    } else {
        64
    }
    .min(inner.width);
    let cols =
        Layout::horizontal([Constraint::Min(1), Constraint::Length(shortcut_width)]).split(inner);

    let block = styles::block("Status");
    frame.render_widget(block, area);
    if cols[0].width < 20 {
        frame.render_widget(Paragraph::new(shortcuts).alignment(Alignment::Left), inner);
        return;
    }
    frame.render_widget(
        Paragraph::new(left_text).alignment(Alignment::Left),
        cols[0],
    );
    frame.render_widget(
        Paragraph::new(shortcuts).alignment(Alignment::Right),
        cols[1],
    );
}

fn render_help_view(frame: &mut Frame, area: Rect) {
    let help = vec![
        styles::gradient_text("GitLab Runner TUI", (125, 207, 255), (187, 154, 247)),
        Line::from(""),
        Line::from(Span::styled("Navigation", styles::accent_style())),
        Line::from(vec![
            Span::styled("  Tab / Shift+Tab", styles::accent_style()),
            Span::raw("  Switch top-level views and load them"),
        ]),
        Line::from(vec![
            Span::styled("  1-7", styles::accent_style()),
            Span::raw("              Jump directly to a view and load it"),
        ]),
        Line::from(vec![
            Span::styled("  ↑/↓ or j/k", styles::accent_style()),
            Span::raw("       Move table selection"),
        ]),
        Line::from(""),
        Line::from(Span::styled("Actions", styles::accent_style())),
        Line::from(vec![
            Span::styled("  Enter", styles::accent_style()),
            Span::raw("            Open selected runner in browser (GitLab admin page)"),
        ]),
        Line::from(vec![
            Span::styled("  r", styles::accent_style()),
            Span::raw("                Refresh the active tab"),
        ]),
        Line::from(vec![
            Span::styled("  p", styles::accent_style()),
            Span::raw("                Toggle polling / auto-refresh"),
        ]),
        Line::from(vec![
            Span::styled("  f or /", styles::accent_style()),
            Span::raw("           Open filter popup (tags + versions multi-select)"),
        ]),
        Line::from(vec![
            Span::styled("  a", styles::accent_style()),
            Span::raw("                Edit Stale tab contact cutoff"),
        ]),
        Line::from(vec![
            Span::styled("  t", styles::accent_style()),
            Span::raw("                Edit tag text filter (comma-separated)"),
        ]),
        Line::from(vec![
            Span::styled("  s", styles::accent_style()),
            Span::raw("                Cycle sort mode"),
        ]),
        Line::from(vec![
            Span::styled("  c", styles::accent_style()),
            Span::raw("                Open settings and diagnostics"),
        ]),
        Line::from(vec![
            Span::styled("  q or Ctrl-C", styles::accent_style()),
            Span::raw("      Quit"),
        ]),
        Line::from(""),
        Line::from(Span::styled("Filtering", styles::accent_style())),
        Line::from(vec![
            Span::styled("  t", styles::accent_style()),
            Span::raw("                Focus the tag text filter bar"),
        ]),
        Line::from(vec![
            Span::styled("  Type tags", styles::accent_style()),
            Span::raw("        Edit comma-separated tag filters"),
        ]),
        Line::from(vec![
            Span::styled("  Enter", styles::accent_style()),
            Span::raw("            Apply tag filter to the active tab"),
        ]),
        Line::from(vec![
            Span::styled("  Esc", styles::accent_style()),
            Span::raw("              Exit filter editing"),
        ]),
        Line::from(""),
        Line::from(Span::styled("Views", styles::accent_style())),
        Line::from(vec![
            Span::styled("  1", styles::accent_style()),
            Span::raw(" Runners   "),
            Span::styled("2", styles::accent_style()),
            Span::raw(" Health   "),
            Span::styled("3", styles::accent_style()),
            Span::raw(" Offline   "),
            Span::styled("4", styles::accent_style()),
            Span::raw(" Stale"),
        ]),
        Line::from(vec![
            Span::styled("  5", styles::accent_style()),
            Span::raw(" Idle      "),
            Span::styled("6", styles::accent_style()),
            Span::raw(" Rotating "),
            Span::styled("7", styles::accent_style()),
            Span::raw(" Workers"),
        ]),
    ];

    let paragraph = Paragraph::new(help)
        .block(styles::block("Help"))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::GitLabClient;
    use crate::conductor::Conductor;
    use crate::config::{AppConfig, RunnerDiscoveryMode};
    use crate::models::manager::RunnerManager;
    use crate::models::runner::Runner;
    use crate::tui::app::{HealthSummary, ManagerRow};
    use insta::assert_snapshot;
    use ratatui::{backend::TestBackend, buffer::Buffer, Terminal};
    use regex::Regex;
    use std::time::Instant;

    fn test_app() -> App {
        let client = GitLabClient::new(
            "https://gitlab.example.com".to_string(),
            "token".to_string(),
        )
        .expect("client");
        App::new(
            Conductor::new_with_mode(client, RunnerDiscoveryMode::VisibleRunners, vec![]),
            AppConfig::default(),
        )
    }

    fn test_manager(
        id: u64,
        system_id: &str,
        status: &str,
        contacted_at: Option<&str>,
    ) -> RunnerManager {
        RunnerManager {
            id,
            system_id: system_id.to_string(),
            created_at: "2024-01-15T10:30:00.000Z".to_string(),
            contacted_at: contacted_at.map(str::to_string),
            ip_address: Some("10.0.1.10".to_string()),
            status: status.to_string(),
            version: Some("18.8.0".to_string()),
            revision: Some("9ffb4aa0".to_string()),
            platform: None,
            architecture: None,
        }
    }

    fn test_runner(id: u64, status: &str, tags: &[&str], managers: Vec<RunnerManager>) -> Runner {
        Runner {
            id,
            runner_type: "group_type".to_string(),
            active: true,
            paused: false,
            description: Some(format!("Runner {}", id)),
            created_at: Some("2024-01-15T10:30:00.000Z".to_string()),
            ip_address: Some("10.0.1.10".to_string()),
            is_shared: false,
            status: status.to_string(),
            version: Some("18.8.0".to_string()),
            revision: Some("9ffb4aa0".to_string()),
            tag_list: tags.iter().map(|tag| tag.to_string()).collect(),
            managers,
            groups: vec![],
        }
    }

    fn render_to_string(app: &mut App, width: u16, height: u16) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal.draw(|frame| render(app, frame)).expect("draw");
        normalize_dynamic_content(&sanitize_rendered_output(&buffer_to_string(
            terminal.backend().buffer(),
        )))
    }

    fn buffer_to_string(buffer: &Buffer) -> String {
        let mut lines = Vec::new();
        for y in 0..buffer.area.height {
            let mut line = String::new();
            for x in 0..buffer.area.width {
                line.push_str(buffer[(x, y)].symbol());
            }
            lines.push(line.trim_end().to_string());
        }
        lines.join("\n")
    }

    fn sanitize_rendered_output(rendered: &str) -> String {
        let border_chars = Regex::new(r"[│┌┐└┘├┤┬┴┼─█╭╮╯╰]").expect("border regex");
        let spaces = Regex::new(r"\s{2,}").expect("spaces regex");

        rendered
            .lines()
            .filter_map(|line| {
                let stripped = border_chars.replace_all(line, " ");
                let compact = spaces.replace_all(&stripped, " ");
                let compact = compact.trim();
                if compact.is_empty() {
                    None
                } else {
                    Some(compact.to_string())
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn normalize_dynamic_content(rendered: &str) -> String {
        let day_age = Regex::new(r"\b\d+d ago\b").expect("day age regex");
        let short_age = Regex::new(r"\b(?:\d+[smh]|just now)\b").expect("short age regex");
        let refreshed = Regex::new(r"last refresh \d+[smhd] ago").expect("refresh regex");

        let normalized = day_age.replace_all(rendered, "<age>");
        let normalized = short_age.replace_all(&normalized, "<age>");
        refreshed
            .replace_all(&normalized, "last refresh <age>")
            .to_string()
    }

    #[test]
    fn renders_runners_with_last_contact_and_polling() {
        let mut app = test_app();
        app.loaded_tab = Some(Tab::Runners);
        app.filter_input = "prod,linux".to_string();
        app.polling_active = true;
        app.last_refresh_at = Some(Instant::now());
        let runner = test_runner(
            326689,
            "online",
            &["platform", "prod"],
            vec![
                test_manager(255550, "s_new", "online", Some("2024-01-21T09:15:00.000Z")),
                test_manager(255373, "s_old", "offline", Some("2024-01-20T08:15:00.000Z")),
            ],
        );
        app.runners = vec![crate::tui::app::UIRunner {
            formatted_tags: runner.tag_list.join(", "),
            formatted_groups: None,
            runner,
        }];
        app.table_state.select(Some(0));

        let rendered = render_to_string(&mut app, 120, 24);
        assert_snapshot!(rendered, @r"
Dashboard Polling [p]
GitLab Runner TUI ● Live · Prev <age> Next <age>
1 Runners 2 Health 3 Offline 4 Stale 5 Idle 6 Rotating 7 Workers
Runners (1)
ID Status Version Last Contact Tags Mgrs
▶ 326689 ● online 18.8.0 <age> platform, prod 2
Status
READY 1 runners for Runners · Runner 326689 [online] p poll · r refresh · f filter · s sort · c settings ·");
    }

    #[test]
    fn renders_offline_empty_state_without_detail_pane() {
        let mut app = test_app();
        app.select_tab(Tab::Offline);
        app.loaded_tab = Some(Tab::Offline);
        app.filter_input = "alm".to_string();

        let rendered = render_to_string(&mut app, 100, 20);
        assert_snapshot!(rendered, @r"
Dashboard Polling [p]
GitLab Runner TUI Paused · Prev never Next -
1 Runners 2 Health 3 Offline 4 Stale 5 Idle 6 Rotating 7 Workers
Offline (0)
No offline runners matched the current tag filter.
Status
READY 0 runners for Offline · no p poll · r refresh · f filter · s sort · c settings ·");
    }

    #[test]
    fn renders_workers_contacted_view() {
        let mut app = test_app();
        app.select_tab(Tab::Workers);
        app.loaded_tab = Some(Tab::Workers);
        app.manager_rows = vec![ManagerRow {
            runner_id: 326759,
            manager: test_manager(
                256551,
                "s_859060915507",
                "online",
                Some("2024-01-21T09:15:00.000Z"),
            ),
        }];
        app.table_state.select(Some(0));

        let rendered = render_to_string(&mut app, 120, 24);
        assert_snapshot!(rendered, @r"
Dashboard Polling [p]
GitLab Runner TUI Paused · Prev never Next -
1 Runners 2 Health 3 Offline 4 Stale 5 Idle 6 Rotating 7 Workers
Workers (1)
Runner Worker System Status Contacted
▶ 326759 256551 s_859060915507 ● online <age>
Status
READY 1 workers for Workers · Worker 256551 on s_859 p poll · r refresh · f filter · s sort · c settings ·");
    }

    #[test]
    fn renders_health_summary_with_last_contact_column() {
        let mut app = test_app();
        app.select_tab(Tab::Health);
        app.loaded_tab = Some(Tab::Health);
        let runner = test_runner(
            326812,
            "online",
            &["platform", "alm"],
            vec![test_manager(
                255552,
                "s_04070b461d87",
                "online",
                Some("2024-01-21T09:15:00.000Z"),
            )],
        );
        app.runners = vec![crate::tui::app::UIRunner {
            formatted_tags: runner.tag_list.join(", "),
            formatted_groups: None,
            runner,
        }];
        app.health_summary = Some(HealthSummary {
            online_count: 1,
            total_count: 1,
        });
        app.table_state.select(Some(0));

        let rendered = render_to_string(&mut app, 120, 24);
        assert_snapshot!(rendered, @r"
Dashboard Polling [p]
GitLab Runner TUI Paused · Prev never Next -
1 Runners 2 Health 3 Offline 4 Stale 5 Idle 6 Rotating 7 Workers
Health Summary
✓ 1 of 1 runners online (100.0%)
Health (1/1 online, 100.0%)
ID Status Version Last Contact Mgrs
▶ 326812 ● online 18.8.0 <age> 1
Status
READY 1 runners for Health · Runner 326812 [online] p poll · r refresh · f filter · s sort · c settings ·");
    }

    #[test]
    fn status_bar_shows_refresh_feedback_while_loading() {
        let mut app = test_app();
        app.filter_input = "prod,linux".to_string();
        app.is_loading = true;

        let rendered = render_to_string(&mut app, 220, 18);
        assert!(rendered.contains("Refreshing Runners"));
        assert!(rendered.contains("prod,linux"));
    }

    #[test]
    fn tabs_render_active_tab_counts() {
        let mut app = test_app();
        app.loaded_tab = Some(Tab::Runners);
        app.tab_counts.insert(Tab::Runners, 12);
        app.tab_counts.insert(Tab::Offline, 3);

        let rendered = render_to_string(&mut app, 120, 18);
        assert!(rendered.contains("1 Runners 12"));
        assert!(rendered.contains("3 Offline 3"));
    }

    #[test]
    fn narrow_status_bar_keeps_essential_shortcuts() {
        let mut app = test_app();
        app.loaded_tab = Some(Tab::Runners);
        let runner = test_runner(
            326689,
            "online",
            &["platform"],
            vec![test_manager(
                255550,
                "s_new",
                "online",
                Some("2024-01-21T09:15:00.000Z"),
            )],
        );
        app.runners = vec![crate::tui::app::UIRunner {
            formatted_tags: runner.tag_list.join(", "),
            formatted_groups: None,
            runner,
        }];

        let rendered = render_to_string(&mut app, 72, 18);
        assert!(rendered.contains("refresh"));
        assert!(rendered.contains("filter"));
        assert!(rendered.contains("READY") || rendered.contains("runners for"));
    }
}
