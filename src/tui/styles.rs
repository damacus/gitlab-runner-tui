use ratatui::{
    style::{Color, Modifier, Style},
    symbols,
    text::{Line, Span},
    widgets::{Block, Borders},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChipKind {
    Hotkey,
    Muted,
    StatusOnline,
    StatusWarning,
    StatusError,
}

pub const COLOR_FG: Color = Color::White;
pub const COLOR_MUTED: Color = Color::DarkGray;
pub const COLOR_BORDER: Color = Color::Gray;
pub const COLOR_ACCENT: Color = Color::Cyan;
pub const COLOR_ACCENT_DIM: Color = Color::Blue;
pub const COLOR_SUCCESS: Color = Color::Green;
pub const COLOR_WARNING: Color = Color::Yellow;
pub const COLOR_ERROR: Color = Color::Red;
pub const COLOR_SELECTION_BG: Color = Color::DarkGray;
pub const COLOR_SELECTION_FG: Color = Color::White;

pub fn status_style(status: &str) -> Style {
    match status {
        "online" => Style::default()
            .fg(COLOR_SUCCESS)
            .add_modifier(Modifier::BOLD),
        "offline" => Style::default()
            .fg(COLOR_ERROR)
            .add_modifier(Modifier::BOLD),
        "stale" => Style::default()
            .fg(COLOR_WARNING)
            .add_modifier(Modifier::BOLD),
        _ => Style::default().fg(COLOR_MUTED),
    }
}

pub fn status_line(status: &str) -> Line<'static> {
    let (symbol, style) = match status {
        "online" => ("●", status_style("online")),
        "offline" => ("✖", status_style("offline")),
        "stale" => ("⚠", status_style("stale")),
        _ => ("○", status_style("other")),
    };

    Line::from(vec![
        Span::styled(format!("{} ", symbol), style),
        Span::styled(status.to_string(), style),
    ])
}

pub fn muted_style() -> Style {
    Style::default().fg(COLOR_MUTED)
}

pub fn accent_style() -> Style {
    Style::default()
        .fg(COLOR_ACCENT)
        .add_modifier(Modifier::BOLD)
}

fn chip_style(kind: ChipKind) -> Style {
    match kind {
        ChipKind::Hotkey => Style::default()
            .fg(Color::Black)
            .bg(COLOR_ACCENT)
            .add_modifier(Modifier::BOLD),
        ChipKind::Muted => Style::default().fg(COLOR_FG).bg(Color::DarkGray),
        ChipKind::StatusOnline => Style::default()
            .fg(Color::Black)
            .bg(COLOR_SUCCESS)
            .add_modifier(Modifier::BOLD),
        ChipKind::StatusWarning => Style::default()
            .fg(Color::Black)
            .bg(COLOR_WARNING)
            .add_modifier(Modifier::BOLD),
        ChipKind::StatusError => Style::default()
            .fg(COLOR_FG)
            .bg(COLOR_ERROR)
            .add_modifier(Modifier::BOLD),
    }
}

pub fn chip(label: impl Into<String>, kind: ChipKind) -> Span<'static> {
    Span::styled(format!(" {} ", label.into()), chip_style(kind))
}

pub fn hotkey_chip(label: impl Into<String>) -> Span<'static> {
    chip(label, ChipKind::Hotkey)
}

pub fn muted_chip(label: impl Into<String>) -> Span<'static> {
    chip(label, ChipKind::Muted)
}

pub fn status_chip(label: impl Into<String>, status: &str) -> Span<'static> {
    let kind = match status {
        "online" | "live" | "ok" => ChipKind::StatusOnline,
        "stale" | "paused" | "warning" => ChipKind::StatusWarning,
        "offline" | "error" => ChipKind::StatusError,
        _ => ChipKind::Muted,
    };
    chip(label, kind)
}

pub fn soft_badge(
    label: impl Into<String>,
    fg: Color,
    bg: Color,
    emphasized: bool,
) -> Span<'static> {
    let style = if emphasized {
        Style::default().fg(fg).bg(bg).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(fg).bg(bg)
    };
    Span::styled(format!(" {} ", label.into()), style)
}

pub fn block(title: impl Into<String>) -> Block<'static> {
    let title = title.into();
    Block::default()
        .borders(Borders::ALL)
        .border_set(symbols::border::ONE_EIGHTH_TALL)
        .border_style(Style::default().fg(COLOR_BORDER))
        .title(Span::styled(
            format!(" {} ", title),
            Style::default().fg(COLOR_FG).add_modifier(Modifier::BOLD),
        ))
}

pub fn focused_block(title: impl Into<String>) -> Block<'static> {
    let title = title.into();
    Block::default()
        .borders(Borders::ALL)
        .border_set(symbols::border::ONE_EIGHTH_TALL)
        .border_style(Style::default().fg(COLOR_ACCENT))
        .title(Span::styled(
            format!(" {} ", title),
            Style::default()
                .fg(COLOR_ACCENT)
                .add_modifier(Modifier::BOLD),
        ))
}

pub fn selected_row_style() -> Style {
    Style::default()
        .bg(COLOR_SELECTION_BG)
        .fg(COLOR_SELECTION_FG)
}

pub fn table_header_style() -> Style {
    Style::default()
        .fg(COLOR_ACCENT)
        .add_modifier(Modifier::BOLD)
}

pub fn sort_column_style() -> Style {
    Style::default().fg(COLOR_ACCENT_DIM)
}

pub fn error_block(title: impl Into<String>) -> Block<'static> {
    let title = title.into();
    Block::default()
        .borders(Borders::ALL)
        .border_set(symbols::border::ONE_EIGHTH_TALL)
        .border_style(Style::default().fg(COLOR_ERROR))
        .title(Span::styled(
            format!(" {} ", title),
            Style::default()
                .fg(COLOR_ERROR)
                .add_modifier(Modifier::BOLD),
        ))
}

pub fn gradient_text(text: &str, start_rgb: (u8, u8, u8), end_rgb: (u8, u8, u8)) -> Line<'static> {
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    if n == 0 {
        return Line::from(vec![]);
    }
    if n == 1 {
        return Line::from(vec![Span::styled(
            text.to_string(),
            Style::default().fg(Color::Rgb(start_rgb.0, start_rgb.1, start_rgb.2)),
        )]);
    }

    let mut spans = Vec::with_capacity(n);
    for (i, &c) in chars.iter().enumerate() {
        let ratio = i as f32 / (n - 1) as f32;
        let r = (start_rgb.0 as f32 + (end_rgb.0 as f32 - start_rgb.0 as f32) * ratio) as u8;
        let g = (start_rgb.1 as f32 + (end_rgb.1 as f32 - start_rgb.1 as f32) * ratio) as u8;
        let b = (start_rgb.2 as f32 + (end_rgb.2 as f32 - start_rgb.2 as f32) * ratio) as u8;
        spans.push(Span::styled(
            c.to_string(),
            Style::default()
                .fg(Color::Rgb(r, g, b))
                .add_modifier(Modifier::BOLD),
        ));
    }
    Line::from(spans)
}
