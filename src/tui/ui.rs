use crate::config::RunnerDiscoveryMode;
use crate::models::runner::RunnerSortKey;
use crate::tui::{
    app::{
        detail_layout_mode, latest_runner_contact_detail, latest_runner_contact_label,
        manager_contact_detail, manager_contact_label, App, AppMode, DetailLayoutMode,
        FilterPopupSection, PollDisplayState, ResultsViewType, SettingsField, Tab,
    },
    styles,
};
use chrono::Utc;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    text::{Line, Span},
    widgets::{
        Cell, Clear, Gauge, List, ListItem, Paragraph, Row, Scrollbar, ScrollbarOrientation,
        ScrollbarState, Table, Tabs, Wrap,
    },
    Frame,
};

fn dash_or(value: &Option<String>) -> String {
    value.as_deref().unwrap_or("-").to_string()
}

fn display_or_dash(value: &str) -> &str {
    if value.trim().is_empty() {
        "-"
    } else {
        value.trim()
    }
}

pub fn render(app: &mut App, frame: &mut Frame) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(3),
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

    match app.mode {
        AppMode::AgeInput => render_age_filter_modal(app, frame),
        AppMode::Settings => render_settings_modal(app, frame),
        AppMode::FilterPopup => render_filter_popup(app, frame),
        _ => {}
    }
}

fn centered_rect(width_percent: u16, height_percent: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - height_percent) / 2),
            Constraint::Percentage(height_percent),
            Constraint::Percentage((100 - height_percent) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - width_percent) / 2),
            Constraint::Percentage(width_percent),
            Constraint::Percentage((100 - width_percent) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn render_header(app: &App, frame: &mut Frame, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(24), Constraint::Length(42)])
        .split(area);

    let title = if app.is_loading {
        format!(
            "GitLab Runner TUI {} {}",
            app.spinner_char(),
            app.active_tab().loading_label()
        )
    } else {
        "GitLab Runner TUI".to_string()
    };

    let header = Paragraph::new(title)
        .style(styles::app_title_style())
        .block(styles::block("Dashboard"));
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
        PollDisplayState::Refreshing => format!("{} Refreshing", app.spinner_char()),
        PollDisplayState::Live => "Live (p to pause)".to_string(),
        PollDisplayState::Paused => "Paused (p to resume)".to_string(),
        PollDisplayState::TimedOut => "Timed out (p to resume)".to_string(),
        PollDisplayState::Error => "Error (p to retry)".to_string(),
    };

    let age = app
        .last_refresh_age_secs()
        .map(format_age)
        .unwrap_or_else(|| "never".to_string());
    let next = app
        .next_poll_in_secs()
        .map(format_age)
        .unwrap_or_else(|| "-".to_string());

    let ratio = match state {
        PollDisplayState::Paused => 0.0,
        _ => app.poll_progress_ratio(),
    };
    let gauge = Gauge::default()
        .label(format!("{}  last {}  next {}", badge, age, next))
        .gauge_style(match state {
            PollDisplayState::Live => styles::accent_style(),
            PollDisplayState::Refreshing => styles::accent_style(),
            PollDisplayState::Paused => styles::muted_style(),
            PollDisplayState::TimedOut => styles::status_style("stale"),
            PollDisplayState::Error => styles::status_style("offline"),
        })
        .ratio(ratio.clamp(0.0, 1.0));
    frame.render_widget(gauge, inner);
}

fn format_age(seconds: u64) -> String {
    match seconds {
        0..=89 => format!("{}s", seconds),
        90..=3599 => format!("{}m", seconds / 60),
        3600..=86_399 => format!("{}h", seconds / 3600),
        _ => format!("{}d", seconds / 86_400),
    }
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
        "Filter Tags [t] (focused)"
    } else if app.mode == AppMode::AgeInput {
        "Age Filter [a] (focused)"
    } else {
        "Filter Tags [t]"
    };

    let has_text = !app.filter_input.is_empty();
    let has_popup_tags = !app.selected_tags.is_empty();

    let (text, style) = if !has_text && !has_popup_tags {
        (
            format!(
                "Press t to edit tags. a: age {}. f: filter. s: sort {}. c: settings.",
                app.age_filter_summary(),
                app.sort_label()
            ),
            styles::muted_style(),
        )
    } else {
        let tag_part = match (has_text, has_popup_tags) {
            (true, true) => format!(
                "{} +{} popup tags",
                app.filter_input,
                app.selected_tags.len()
            ),
            (true, false) => app.filter_input.clone(),
            (false, true) => format!("{} popup tags", app.selected_tags.len()),
            (false, false) => unreachable!(),
        };
        (
            format!(
                "{} | age {} | versions {} | sort {}",
                tag_part,
                app.age_filter_summary(),
                app.selected_versions_summary(),
                app.sort_label()
            ),
            styles::accent_style(),
        )
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
    } else if app.mode == AppMode::AgeInput {
        frame.set_cursor(
            area.x + app.age_filter_input.chars().count() as u16 + 1,
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

    if app.active_tab() == Tab::Empty {
        render_runner_table(app, frame, area, false);
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
                Cell::from(overall_status).style(styles::status_style(overall_status)),
            ])
        } else {
            let status =
                Cell::from(runner.status.as_str()).style(styles::status_style(&runner.status));
            let last_contact = Cell::from(latest_runner_contact_label(runner, now));

            match active_tab {
                Tab::Runners => Row::new(vec![
                    Cell::from(runner.id.to_string()),
                    status,
                    Cell::from(dash_or(&runner.version)),
                    last_contact,
                    Cell::from(runner.tag_list.join(", ")),
                    Cell::from(runner.managers.len().to_string()),
                ]),
                Tab::Health => Row::new(vec![
                    Cell::from(runner.id.to_string()),
                    status,
                    Cell::from(dash_or(&runner.version)),
                    last_contact,
                    Cell::from(runner.managers.len().to_string()),
                ]),
                Tab::Offline | Tab::Uncontacted => Row::new(vec![
                    Cell::from(runner.id.to_string()),
                    Cell::from(dash_or(&runner.version)),
                    last_contact,
                    Cell::from(runner.managers.len().to_string()),
                ]),
                Tab::Empty => Row::new(vec![
                    Cell::from(runner.id.to_string()),
                    Cell::from(dash_or(&runner.version)),
                    status,
                ]),
                _ => Row::new(vec![
                    Cell::from(runner.id.to_string()),
                    status,
                    Cell::from(dash_or(&runner.version)),
                    last_contact,
                    Cell::from(runner.tag_list.join(", ")),
                    Cell::from(runner.managers.len().to_string()),
                ]),
            }
        }
    });

    let widths = if rotating {
        vec![
            Constraint::Length(8),
            Constraint::Percentage(36),
            Constraint::Percentage(36),
            Constraint::Length(10),
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

    let table = Table::new(rows, widths)
        .header(header)
        .highlight_style(styles::selected_row_style())
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
            Cell::from(row.runner_id.to_string()),
            Cell::from(row.manager.id.to_string()),
            Cell::from(row.manager.system_id.as_str()),
            Cell::from(row.manager.status.as_str())
                .style(styles::status_style(&row.manager.status)),
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
    )
    .header(header)
    .highlight_style(styles::selected_row_style())
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
    let Some(runner) = app.selected_runner() else {
        let paragraph = Paragraph::new("Select a runner to inspect its details.")
            .style(styles::muted_style())
            .block(styles::block("Details"));
        frame.render_widget(paragraph, area);
        return;
    };

    let mut items = vec![
        ListItem::new(format!("ID: {}", runner.id)),
        ListItem::new(format!(
            "Last Contact: {}",
            latest_runner_contact_detail(runner)
        )),
        ListItem::new(format!(
            "Version: {}",
            runner.version.as_deref().unwrap_or("-")
        )),
        ListItem::new(format!(
            "Revision: {}",
            runner.revision.as_deref().unwrap_or("-")
        )),
        ListItem::new(format!(
            "Tags: {}",
            if runner.tag_list.is_empty() {
                "-".to_string()
            } else {
                runner.tag_list.join(", ")
            }
        )),
    ];

    if !runner.runner_type.is_empty() {
        let display_type = match runner.runner_type.as_str() {
            "project_type" => "project",
            "group_type" => "group",
            "instance_type" => "instance",
            other => other,
        };
        items.push(ListItem::new(format!("Type: {display_type}")));
    }

    if let Some(ip_address) = &runner.ip_address {
        items.push(ListItem::new(format!("IP: {}", ip_address)));
    }

    if let Some(description) = &runner.description {
        items.push(ListItem::new(format!("Description: {}", description)));
    }

    if !runner.managers.is_empty() {
        items.push(ListItem::new(" "));
        items.push(ListItem::new("Managers:").style(styles::accent_style()));
        for manager in &runner.managers {
            items.push(ListItem::new(format!(
                "{} [{}] {} {}",
                manager.system_id,
                manager.status,
                manager_contact_label(manager, Utc::now()),
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

fn render_age_filter_modal(app: &App, frame: &mut Frame) {
    let area = centered_rect(52, 22, frame.size());
    frame.render_widget(Clear, area);
    let block = styles::focused_block("Age Filter [a]");
    let inner = block.inner(area);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from("Type a minimum runner age like 24h, 7d, or 90m."),
            Line::from(""),
            Line::from(format!(
                "Older than: {}",
                if app.age_filter_input.trim().is_empty() {
                    "any".to_string()
                } else {
                    app.age_filter_input.clone()
                }
            )),
            Line::from(""),
            Line::from("Enter: apply  Esc: cancel"),
        ])
        .block(block),
        area,
    );
    frame.set_cursor(
        inner.x + "Older than: ".len() as u16 + app.age_filter_input.len() as u16,
        inner.y + 2,
    );
}

fn render_filter_popup(app: &mut App, frame: &mut Frame) {
    let area = centered_rect(55, 65, frame.size());
    frame.render_widget(Clear, area);

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(47),
            Constraint::Percentage(47),
            Constraint::Length(1),
        ])
        .split(area);

    // --- Tags section ---
    let tags_focused = app.filter_popup_section == FilterPopupSection::Tags;
    let tags_title = if tags_focused { "▶ Tags" } else { "  Tags" };
    let tag_items: Vec<ListItem> = if app.tag_options.is_empty() {
        vec![ListItem::new("No tags discovered yet.")]
    } else {
        app.tag_options
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
        frame.render_stateful_widget(tags_list, sections[0], &mut app.tag_list_state);
    } else {
        frame.render_widget(tags_list, sections[0]);
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
        frame.render_stateful_widget(versions_list, sections[1], &mut app.version_list_state);
    } else {
        frame.render_widget(versions_list, sections[1]);
    }

    // --- Footer ---
    let footer = Paragraph::new("space:toggle  tab:switch section  a:clear section  esc:close")
        .style(styles::muted_style());
    frame.render_widget(footer, sections[2]);
}

fn render_settings_modal(app: &mut App, frame: &mut Frame) {
    let area = centered_rect(72, 70, frame.size());
    frame.render_widget(Clear, area);

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
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
            "Age filter: {} | Versions: {} | Sort: {}",
            app.age_filter_summary(),
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
                    "/runners/all → /runners (fallback)"
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
    let mut status = match app.mode {
        AppMode::Help => "Any key closes help".to_string(),
        AppMode::FilterInput => {
            "Type tags | Enter: apply filter and refresh | Esc: stop editing | Ctrl-C: quit"
                .to_string()
        }
        AppMode::AgeInput => {
            "Type runner age like 24h, 7d, 90m | Enter: apply age filter | Esc: cancel"
                .to_string()
        }
        AppMode::FilterPopup => {
            "Filter | f/esc close | tab switch section | space toggle | a clear section"
                .to_string()
        }
        AppMode::Settings => {
            "Settings | Tab/Shift+Tab move | Type edit | ←/→ discovery | Ctrl-S save | b benchmark | Esc close"
                .to_string()
        }
        AppMode::Dashboard => {
            if app.is_loading {
                let filter = if app.filter_input.trim().is_empty() {
                    "all runners".to_string()
                } else {
                    format!("tags [{}]", app.filter_input.trim())
                };
                format!(
                    "{} Refreshing {} using {}",
                    app.spinner_char(),
                    app.active_tab(),
                    filter
                )
            } else if let Some(error) = &app.error_message {
                format!(
                    "Last refresh failed for {}: {}",
                    app.active_tab(),
                    error.lines().next().unwrap_or("unknown error")
                )
            } else {
                let result_label = match app.current_results_view_type() {
                    ResultsViewType::Workers => "workers",
                    _ => "runners",
                };
                let refreshed = app
                    .last_refresh_age_secs()
                    .map(|age| format!("last refresh {} ago", format_age(age)))
                    .unwrap_or_else(|| "not loaded yet".to_string());
                format!(
                    "{} {} loaded for {} | {} | f filter | t tags | a age | s sort | c settings | r refresh | p poll | ?: help | q/Ctrl-C quit",
                    app.active_result_len(),
                    result_label,
                    app.active_tab(),
                    refreshed,
                )
            }
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
        Line::from("  Tab / Shift+Tab  Switch top-level views and load them"),
        Line::from("  1-7              Jump directly to a view and load it"),
        Line::from("  ↑/↓ or j/k       Move table selection"),
        Line::from(""),
        Line::from("Actions"),
        Line::from("  Enter            Apply the current filter"),
        Line::from("  r                Refresh the active tab"),
        Line::from("  p                Toggle polling / auto-refresh"),
        Line::from("  a                Edit age filter (24h, 7d, 90m)"),
        Line::from("  f or /           Open filter popup (tags + versions multi-select)"),
        Line::from("  t                Edit tag text filter (comma-separated)"),
        Line::from("  s                Cycle sort mode"),
        Line::from("  c                Open settings and diagnostics"),
        Line::from("  q or Ctrl-C      Quit"),
        Line::from(""),
        Line::from("Filtering"),
        Line::from("  t                Focus the tag text filter bar"),
        Line::from("  Type tags        Edit comma-separated tag filters"),
        Line::from("  Enter            Apply tag filter to the active tab"),
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
                line.push_str(buffer.get(x, y).symbol());
            }
            lines.push(line.trim_end().to_string());
        }
        lines.join("\n")
    }

    fn sanitize_rendered_output(rendered: &str) -> String {
        let border_chars = Regex::new(r"[│┌┐└┘├┤┬┴┼─█]").expect("border regex");
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
        app.runners = vec![test_runner(
            326689,
            "online",
            &["platform", "prod"],
            vec![
                test_manager(255550, "s_new", "online", Some("2024-01-21T09:15:00.000Z")),
                test_manager(255373, "s_old", "offline", Some("2024-01-20T08:15:00.000Z")),
            ],
        )];
        app.table_state.select(Some(0));

        let rendered = render_to_string(&mut app, 120, 24);
        assert_snapshot!(rendered, @r"
Dashboard Polling [p]
GitLab Runner TUI Live (p to pause) last <age> next <age>
Views
1 Runners | 2 Health | 3 Offline | 4 Uncontacted | 5 Empty | 6 Rotating | 7 Workers
Filter Tags [t]
prod,linux | age -h | versions All versions | sort None
Runners (1)
ID Status Version Last Contact Tags Mgrs
326689 online 18.8.0 <age> platform, prod 2
Status
1 runners loaded for Runners | last refresh <age> ago | f filter | t tags | a age | s sort | c settings | r refresh | p p");
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
GitLab Runner TUI Paused (p to resume) last never next -
Views
1 Runners | 2 Health | 3 Offline | 4 Uncontacted | 5 Empty | 6 Rotating | 7 Workers
Filter Tags [t]
alm | age -h | versions All versions | sort None
Offline (0)
No offline runners matched the current tag filter.
Status
0 runners loaded for Offline | not loaded yet | f filter | t tags | a age | s sort | c settings |");
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
GitLab Runner TUI Paused (p to resume) last never next -
Views
1 Runners | 2 Health | 3 Offline | 4 Uncontacted | 5 Empty | 6 Rotating | 7 Workers
Filter Tags [t]
Press t to edit tags. a: age -h. f: filter. s: sort None. c: settings.
Workers (1)
Runner Worker System Status Contacted
326759 256551 s_859060915507 online <age>
Status
1 workers loaded for Workers | not loaded yet | f filter | t tags | a age | s sort | c settings | r refresh | p poll |");
    }

    #[test]
    fn renders_health_summary_with_last_contact_column() {
        let mut app = test_app();
        app.select_tab(Tab::Health);
        app.loaded_tab = Some(Tab::Health);
        app.runners = vec![test_runner(
            326812,
            "online",
            &["platform", "alm"],
            vec![test_manager(
                255552,
                "s_04070b461d87",
                "online",
                Some("2024-01-21T09:15:00.000Z"),
            )],
        )];
        app.health_summary = Some(HealthSummary {
            online_count: 1,
            total_count: 1,
        });
        app.table_state.select(Some(0));

        let rendered = render_to_string(&mut app, 120, 24);
        assert_snapshot!(rendered, @r"
Dashboard Polling [p]
GitLab Runner TUI Paused (p to resume) last never next -
Views
1 Runners | 2 Health | 3 Offline | 4 Uncontacted | 5 Empty | 6 Rotating | 7 Workers
Filter Tags [t]
Press t to edit tags. a: age -h. f: filter. s: sort None. c: settings.
Health Summary
✓ 1 of 1 runners online (100.0%)
Health (1/1 online, 100.0%)
ID Status Version Last Contact Mgrs
326812 online 18.8.0 <age> 1
Status
1 runners loaded for Health | not loaded yet | f filter | t tags | a age | s sort | c settings | r refresh | p poll |");
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
}
