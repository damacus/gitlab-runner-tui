use ratatui::{
    style::{Color, Modifier, Style},
    widgets::{Block, Borders},
};

pub const COLOR_BG: Color = Color::Black;
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

pub fn app_title_style() -> Style {
    Style::default()
        .fg(COLOR_FG)
        .bg(COLOR_BG)
        .add_modifier(Modifier::BOLD)
}

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

pub fn muted_style() -> Style {
    Style::default().fg(COLOR_MUTED)
}

pub fn accent_style() -> Style {
    Style::default()
        .fg(COLOR_ACCENT)
        .add_modifier(Modifier::BOLD)
}

pub fn block(title: impl Into<String>) -> Block<'static> {
    let title = title.into();
    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(COLOR_BORDER))
        .title(title)
}

pub fn focused_block(title: impl Into<String>) -> Block<'static> {
    let title = title.into();
    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(COLOR_ACCENT))
        .title(title)
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
        .border_style(Style::default().fg(COLOR_ERROR))
        .title(title)
}
