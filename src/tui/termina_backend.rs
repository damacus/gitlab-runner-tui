// SPDX-License-Identifier: MIT
// Adapted from ratatui-termina at 54b6874357764219168c63eef2da26935b9a9609.
// Copyright (c) 2016-2022 Florian Dehau; 2023-2025 The Ratatui Developers.
// The full upstream notice is in LICENSES/ratatui-termina.txt.
//! Termina 0.4 rendering support until a compatible backend is published.
//! Keeping this backend in the application makes crates.io packages independent
//! of Cargo Git patches, while terminal input and lifecycle remain in main.

use std::fmt::{self, Write as FmtWrite};
use std::io::{self, Write};

use ratatui::backend::{Backend, ClearType, WindowSize};
use ratatui::buffer::{Cell, CellWidth};
use ratatui::layout::{Position, Size};
use ratatui::style::{Color, Modifier};
use termina::escape::csi::{
    Csi, Cursor, DecPrivateMode, DecPrivateModeCode, Edit, EraseInDisplay, EraseInLine, Mode, Sgr,
    SgrAttributes, SgrModifiers,
};
use termina::style::{ColorSpec, RgbColor};
use termina::{Event, OneBased, Terminal};

macro_rules! decset {
    ($mode:ident) => {{
        let mode = DecPrivateMode::Code(DecPrivateModeCode::$mode);
        Csi::Mode(Mode::SetDecPrivateMode(mode))
    }};
}

macro_rules! decreset {
    ($mode:ident) => {{
        let mode = DecPrivateMode::Code(DecPrivateModeCode::$mode);
        Csi::Mode(Mode::ResetDecPrivateMode(mode))
    }};
}

/// A [`Backend`] implementation that renders through a [`termina::Terminal`].
///
/// `TerminaBackend` writes typed Termina escape sequences for drawing, cursor control, clearing,
/// and terminal-size queries.
///
/// This backend does not automatically enable raw mode or switch to the alternate screen. Use
/// Termina's terminal APIs and typed escape sequences to configure those application-level modes
/// before drawing.
///
pub(crate) struct TerminaBackend<T>
where
    T: Terminal,
{
    terminal: T,
}

impl<T> TerminaBackend<T>
where
    T: Terminal,
{
    /// Creates a backend that writes to the given Termina terminal.
    pub const fn new(terminal: T) -> Self {
        Self { terminal }
    }

    /// Returns the wrapped terminal.
    pub const fn terminal(&self) -> &T {
        &self.terminal
    }

    /// Returns the wrapped terminal as a mutable reference.
    ///
    /// Direct writes can desynchronize Ratatui's diffing buffers from the visible terminal. Clear
    /// the terminal or force a full redraw before relying on Ratatui's next diff.
    pub const fn terminal_mut(&mut self) -> &mut T {
        &mut self.terminal
    }
}

impl<T> Write for TerminaBackend<T>
where
    T: Terminal,
{
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.terminal.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.terminal.flush()
    }
}

impl<T> Backend for TerminaBackend<T>
where
    T: Terminal,
{
    type Error = io::Error;

    fn draw<'a, I>(&mut self, content: I) -> io::Result<()>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        let mut string = String::with_capacity(content.size_hint().0 * 3);
        let mut fg = Color::Reset;
        let mut bg = Color::Reset;

        let mut underline_color = Color::Reset;
        let mut modifier = Modifier::empty();
        // Position and width of the last cell written, used to skip redundant cursor moves.
        let mut last: Option<(Position, u16)> = None;
        for (x, y, cell) in content {
            // Move the cursor unless it already sits at (x, y), i.e. this cell directly follows
            // the previous one on the same row, accounting for the width of what was printed.
            if !matches!(last, Some((p, w)) if p.x.checked_add(w) == Some(x) && y == p.y) {
                let command = Csi::Cursor(cursor_position(Position { x, y })?);
                write!(string, "{command}").unwrap();
            }
            let width = cell.cell_width();
            // A VS16 cluster's terminal cursor advance is not reliably predictable.
            let uncertain_width = width > 1 && cell.symbol().chars().any(|c| c == '\u{FE0F}');
            last = (!uncertain_width).then_some((Position { x, y }, width));

            let mut attributes = SgrAttributes::default();
            if cell.fg != fg {
                attributes.foreground = Some(cell.fg.into_termina());
                fg = cell.fg;
            }
            if cell.bg != bg {
                attributes.background = Some(cell.bg.into_termina());
                bg = cell.bg;
            }

            if cell.underline_color != underline_color {
                attributes.underline_color = Some(cell.underline_color.into_termina());
                underline_color = cell.underline_color;
            }
            if cell.modifier != modifier {
                attributes.modifiers = ModifierDiff {
                    from: modifier,
                    to: cell.modifier,
                }
                .into_termina();
                modifier = cell.modifier;
            }
            if !attributes.is_empty() {
                write!(string, "{}", Csi::Sgr(Sgr::Attributes(attributes))).unwrap();
            }

            string.push_str(cell.symbol());
        }

        write!(self.terminal, "{string}{}", Csi::Sgr(Sgr::Reset))
    }

    fn hide_cursor(&mut self) -> io::Result<()> {
        let command = decreset!(ShowCursor);
        write!(self.terminal, "{command}")?;
        self.terminal.flush()
    }

    fn show_cursor(&mut self) -> io::Result<()> {
        let command = decset!(ShowCursor);
        write!(self.terminal, "{command}")?;
        self.terminal.flush()
    }

    fn get_cursor_position(&mut self) -> io::Result<Position> {
        let command = Csi::Cursor(Cursor::RequestActivePositionReport);
        write!(self.terminal, "{command}")?;
        self.terminal.flush()?;
        let event = self.terminal.read(|event| {
            matches!(
                event,
                Event::Csi(Csi::Cursor(Cursor::ActivePositionReport { .. }))
            )
        })?;
        let Event::Csi(Csi::Cursor(Cursor::ActivePositionReport { line, col })) = event else {
            return Err(io::Error::other(
                "termina returned a non-cursor-position event",
            ));
        };
        Ok(Position {
            x: col.get_zero_based(),
            y: line.get_zero_based(),
        })
    }

    fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> io::Result<()> {
        let command = Csi::Cursor(cursor_position(position.into())?);
        write!(self.terminal, "{command}")?;
        self.terminal.flush()
    }

    fn clear(&mut self) -> io::Result<()> {
        self.clear_region(ClearType::All)
    }

    fn clear_region(&mut self, clear_type: ClearType) -> io::Result<()> {
        let edit = match clear_type {
            ClearType::All => Edit::EraseInDisplay(EraseInDisplay::EraseDisplay),
            ClearType::AfterCursor => Edit::EraseInDisplay(EraseInDisplay::EraseToEndOfDisplay),
            ClearType::BeforeCursor => Edit::EraseInDisplay(EraseInDisplay::EraseToStartOfDisplay),
            ClearType::CurrentLine => Edit::EraseInLine(EraseInLine::EraseLine),
            ClearType::UntilNewLine => Edit::EraseInLine(EraseInLine::EraseToEndOfLine),
        };
        let command = Csi::Edit(edit);
        write!(self.terminal, "{command}")?;
        self.terminal.flush()
    }

    fn append_lines(&mut self, n: u16) -> io::Result<()> {
        for _ in 0..n {
            writeln!(self.terminal)?;
        }
        self.terminal.flush()
    }

    fn size(&self) -> io::Result<Size> {
        let size = self.terminal.get_dimensions()?;
        Ok(Size::new(size.cols, size.rows))
    }

    fn window_size(&mut self) -> io::Result<WindowSize> {
        let size = self.terminal.get_dimensions()?;
        Ok(WindowSize {
            columns_rows: Size::new(size.cols, size.rows),
            pixels: Size::new(
                size.pixel_width.unwrap_or_default(),
                size.pixel_height.unwrap_or_default(),
            ),
        })
    }

    fn flush(&mut self) -> io::Result<()> {
        self.terminal.flush()
    }
}

fn cursor_position(position: Position) -> io::Result<Cursor> {
    Ok(Cursor::Position {
        line: one_based(position.y)?,
        col: one_based(position.x)?,
    })
}

fn one_based(n: u16) -> io::Result<OneBased> {
    n.checked_add(1)
        .and_then(OneBased::new)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "position exceeds u16::MAX - 1"))
}

/// A trait for converting a Ratatui type to a Termina type.
trait IntoTermina<T> {
    /// Converts the ratatui type to a Termina type.
    fn into_termina(self) -> T;
}

struct ModifierDiff {
    from: Modifier,
    to: Modifier,
}

impl IntoTermina<ColorSpec> for Color {
    fn into_termina(self) -> ColorSpec {
        match self {
            Self::Reset => ColorSpec::Reset,
            Self::Black => ColorSpec::BLACK,
            Self::Red => ColorSpec::RED,
            Self::Green => ColorSpec::GREEN,
            Self::Yellow => ColorSpec::YELLOW,
            Self::Blue => ColorSpec::BLUE,
            Self::Magenta => ColorSpec::MAGENTA,
            Self::Cyan => ColorSpec::CYAN,
            Self::Gray => ColorSpec::WHITE,
            Self::DarkGray => ColorSpec::BRIGHT_BLACK,
            Self::LightRed => ColorSpec::BRIGHT_RED,
            Self::LightGreen => ColorSpec::BRIGHT_GREEN,
            Self::LightYellow => ColorSpec::BRIGHT_YELLOW,
            Self::LightBlue => ColorSpec::BRIGHT_BLUE,
            Self::LightMagenta => ColorSpec::BRIGHT_MAGENTA,
            Self::LightCyan => ColorSpec::BRIGHT_CYAN,
            Self::White => ColorSpec::BRIGHT_WHITE,
            Self::Indexed(i) => ColorSpec::PaletteIndex(i),
            Self::Rgb(r, g, b) => ColorSpec::TrueColor(RgbColor::new(r, g, b).into()),
        }
    }
}

impl IntoTermina<SgrModifiers> for ModifierDiff {
    fn into_termina(self) -> SgrModifiers {
        let removed = self.from - self.to;
        let added = self.to - self.from;
        let mut modifiers = SgrModifiers::empty();

        if removed.contains(Modifier::BOLD) || removed.contains(Modifier::DIM) {
            modifiers |= SgrModifiers::INTENSITY_NORMAL;
        }
        if removed.contains(Modifier::ITALIC) {
            modifiers |= SgrModifiers::NO_ITALIC;
        }
        if removed.contains(Modifier::UNDERLINED) {
            modifiers |= SgrModifiers::UNDERLINE_NONE;
        }
        if removed.contains(Modifier::SLOW_BLINK) || removed.contains(Modifier::RAPID_BLINK) {
            modifiers |= SgrModifiers::BLINK_NONE;
        }
        if removed.contains(Modifier::REVERSED) {
            modifiers |= SgrModifiers::NO_REVERSE;
        }
        if removed.contains(Modifier::HIDDEN) {
            modifiers |= SgrModifiers::NO_INVISIBLE;
        }
        if removed.contains(Modifier::CROSSED_OUT) {
            modifiers |= SgrModifiers::NO_STRIKE_THROUGH;
        }

        if added.contains(Modifier::BOLD) {
            modifiers |= SgrModifiers::INTENSITY_BOLD;
        }
        if added.contains(Modifier::DIM) {
            modifiers |= SgrModifiers::INTENSITY_DIM;
        }
        if added.contains(Modifier::ITALIC) {
            modifiers |= SgrModifiers::ITALIC;
        }
        if added.contains(Modifier::UNDERLINED) {
            modifiers |= SgrModifiers::UNDERLINE_SINGLE;
        }
        if added.contains(Modifier::SLOW_BLINK) {
            modifiers |= SgrModifiers::BLINK_SLOW;
        }
        if added.contains(Modifier::RAPID_BLINK) {
            modifiers |= SgrModifiers::BLINK_RAPID;
        }
        if added.contains(Modifier::REVERSED) {
            modifiers |= SgrModifiers::REVERSE;
        }
        if added.contains(Modifier::HIDDEN) {
            modifiers |= SgrModifiers::INVISIBLE;
        }
        if added.contains(Modifier::CROSSED_OUT) {
            modifiers |= SgrModifiers::STRIKE_THROUGH;
        }

        modifiers
    }
}

impl<T> fmt::Debug for TerminaBackend<T>
where
    T: Terminal + fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TerminaBackend")
            .field("terminal", &self.terminal)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU16;
    use std::time::Duration;

    use ratatui::buffer::{Cell, CellDiffOption};
    use ratatui::style::Style;
    use termina::escape::csi::Csi;
    use termina::EventReader;

    use super::*;

    #[derive(Debug)]
    struct MockTerminal {
        output: Vec<u8>,
        size: termina::WindowSize,
        events: Vec<Event>,
    }

    impl MockTerminal {
        fn new() -> Self {
            Self {
                output: Vec::new(),
                size: termina::WindowSize {
                    cols: 80,
                    rows: 24,
                    pixel_width: Some(800),
                    pixel_height: Some(480),
                },
                events: Vec::new(),
            }
        }

        fn with_event(mut self, event: Event) -> Self {
            self.events.push(event);
            self
        }

        fn output(&self) -> String {
            String::from_utf8_lossy(&self.output).into_owned()
        }
    }

    impl Write for MockTerminal {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.output.extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl Terminal for MockTerminal {
        fn enter_raw_mode(&mut self) -> io::Result<()> {
            Ok(())
        }

        fn enter_cooked_mode(&mut self) -> io::Result<()> {
            Ok(())
        }

        fn get_dimensions(&self) -> io::Result<termina::WindowSize> {
            Ok(self.size)
        }

        fn event_reader(&self) -> EventReader {
            unimplemented!("backend tests do not use event_reader")
        }

        fn poll<F: Fn(&Event) -> bool>(
            &self,
            filter: F,
            _timeout: Option<Duration>,
        ) -> io::Result<bool> {
            Ok(self.events.iter().any(filter))
        }

        fn read<F: Fn(&Event) -> bool>(&self, filter: F) -> io::Result<Event> {
            self.events
                .iter()
                .find(|event| filter(event))
                .cloned()
                .ok_or_else(|| io::Error::new(io::ErrorKind::WouldBlock, "no matching event"))
        }

        fn set_panic_hook(
            &mut self,
            _f: impl Fn(&mut termina::PlatformHandle) + Send + Sync + 'static,
        ) {
        }
    }

    fn backend() -> TerminaBackend<MockTerminal> {
        TerminaBackend::new(MockTerminal::new())
    }

    #[test]
    fn writes_cursor_visibility_commands() {
        let mut backend = backend();
        backend.hide_cursor().unwrap();
        backend.show_cursor().unwrap();

        let hide_cursor = decreset!(ShowCursor);
        let show_cursor = decset!(ShowCursor);
        assert_eq!(
            backend.terminal.output(),
            format!("{hide_cursor}{show_cursor}")
        );
    }

    #[test]
    fn reads_cursor_position_reports() {
        let event = Event::Csi(Csi::Cursor(Cursor::ActivePositionReport {
            line: OneBased::new(5).unwrap(),
            col: OneBased::new(7).unwrap(),
        }));
        let mut backend = TerminaBackend::new(MockTerminal::new().with_event(event));

        assert_eq!(backend.get_cursor_position().unwrap(), Position::new(6, 4));
        let request = Csi::Cursor(Cursor::RequestActivePositionReport);
        assert_eq!(backend.terminal.output(), request.to_string());
    }

    #[test]
    fn rejects_non_cursor_position_reports() {
        let event = Event::FocusIn;
        let mut backend = TerminaBackend::new(MockTerminal::new().with_event(event));

        let error = backend.get_cursor_position().unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
    }

    #[test]
    fn sets_cursor_position() {
        let mut backend = backend();
        backend.set_cursor_position(Position::new(3, 4)).unwrap();

        let position = Cursor::Position {
            line: OneBased::new(5).unwrap(),
            col: OneBased::new(4).unwrap(),
        };
        assert_eq!(backend.terminal.output(), Csi::Cursor(position).to_string());
    }

    #[test]
    fn rejects_cursor_position_overflow() {
        let mut backend = backend();

        let error = backend.set_cursor_position(Position::new(u16::MAX, 0));
        assert_eq!(error.unwrap_err().kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn clears_regions() {
        let mut backend = backend();
        backend.clear_region(ClearType::All).unwrap();
        backend.clear_region(ClearType::AfterCursor).unwrap();
        backend.clear_region(ClearType::BeforeCursor).unwrap();
        backend.clear_region(ClearType::CurrentLine).unwrap();
        backend.clear_region(ClearType::UntilNewLine).unwrap();

        let expected = [
            Csi::Edit(Edit::EraseInDisplay(EraseInDisplay::EraseDisplay)),
            Csi::Edit(Edit::EraseInDisplay(EraseInDisplay::EraseToEndOfDisplay)),
            Csi::Edit(Edit::EraseInDisplay(EraseInDisplay::EraseToStartOfDisplay)),
            Csi::Edit(Edit::EraseInLine(EraseInLine::EraseLine)),
            Csi::Edit(Edit::EraseInLine(EraseInLine::EraseToEndOfLine)),
        ]
        .into_iter()
        .map(|command| command.to_string())
        .collect::<String>();
        assert_eq!(backend.terminal.output(), expected);
    }

    #[test]
    fn reports_terminal_size() {
        let mut backend = backend();

        assert_eq!(backend.size().unwrap(), Size::new(80, 24));
        assert_eq!(
            backend.window_size().unwrap(),
            WindowSize {
                columns_rows: Size::new(80, 24),
                pixels: Size::new(800, 480),
            }
        );
    }

    #[test]
    fn appends_lines() {
        let mut backend = backend();
        backend.append_lines(3).unwrap();

        assert_eq!(backend.terminal.output(), "\n\n\n");
    }

    #[test]
    fn draws_cells_with_grouped_sgr_attributes() {
        let mut backend = backend();
        let mut cell = Cell::new("x");
        cell.set_style(
            Style::new()
                .fg(Color::Red)
                .bg(Color::Blue)
                .add_modifier(Modifier::BOLD),
        );
        let content = [(2, 3, &cell)];

        backend.draw(content.into_iter()).unwrap();

        let output = backend.terminal.output();
        let cursor = Csi::Cursor(cursor_position(Position::new(2, 3)).unwrap());
        assert!(output.starts_with(&cursor.to_string()));
        assert!(output.contains('x'));
        assert!(output.ends_with(&Csi::Sgr(Sgr::Reset).to_string()));
    }

    #[test]
    fn draws_cursor_move_after_wide_symbol() {
        // A double-width glyph advances the terminal cursor by two columns, so writing the
        // very next cell requires an explicit cursor move. See issue #2651.
        let mut backend = backend();
        let wide = Cell::new("\u{2764}\u{FE0F}"); // ❤️ (VS16 emoji presentation)
        let next = Cell::new("a");
        let content = [(0, 0, &wide), (1, 0, &next)];

        backend.draw(content.into_iter()).unwrap();

        let output = backend.terminal.output();
        let cursor = Csi::Cursor(cursor_position(Position::new(1, 0)).unwrap());
        assert!(
            output.contains(&cursor.to_string()),
            "expected a cursor move to (1, 0) after a wide glyph, got: {output:?}"
        );
    }

    #[test]
    fn skips_cursor_move_for_contiguous_cells() {
        let mut backend = backend();
        let a = Cell::new("a");
        let b = Cell::new("b");
        let content = [(0, 0, &a), (1, 0, &b)];

        backend.draw(content.into_iter()).unwrap();

        let output = backend.terminal.output();
        let cursor = Csi::Cursor(cursor_position(Position::new(1, 0)).unwrap());
        assert!(!output.contains(&cursor.to_string()));
    }

    #[test]
    fn skips_cursor_move_after_wide_symbol_when_contiguous() {
        let mut backend = backend();
        let wide = Cell::new("\u{1F600}"); // 😀
        let next = Cell::new("a");
        let content = [(0, 0, &wide), (2, 0, &next)];

        backend.draw(content.into_iter()).unwrap();

        let output = backend.terminal.output();
        let cursor = Csi::Cursor(cursor_position(Position::new(2, 0)).unwrap());
        assert!(!output.contains(&cursor.to_string()));
    }

    #[test]
    fn moves_cursor_after_uncertain_width_symbol() {
        let mut backend = backend();
        let wide = Cell::new("\u{2764}\u{FE0F}"); // ❤️
        let next = Cell::new("a");
        let content = [(0, 0, &wide), (2, 0, &next)];

        backend.draw(content.into_iter()).unwrap();

        let output = backend.terminal.output();
        let cursor = Csi::Cursor(cursor_position(Position::new(2, 0)).unwrap());
        assert!(output.contains(&cursor.to_string()));
    }

    #[test]
    fn moves_cursor_when_previous_width_overflows() {
        let mut backend = backend();
        let mut forced = Cell::new("a");
        forced.set_diff_option(CellDiffOption::ForcedWidth(NonZeroU16::MAX));
        let next = Cell::new("b");
        let content = [(1, 0, &forced), (0, 0, &next)];

        backend.draw(content.into_iter()).unwrap();

        let output = backend.terminal.output();
        let cursor = Csi::Cursor(cursor_position(Position::new(0, 0)).unwrap());
        assert!(output.contains(&cursor.to_string()));
    }

    #[test]
    fn converts_ratatui_colors_to_termina_colors() {
        assert_eq!(Color::Reset.into_termina(), ColorSpec::Reset);
        assert_eq!(Color::Red.into_termina(), ColorSpec::RED);
        assert_eq!(
            Color::Indexed(42).into_termina(),
            ColorSpec::PaletteIndex(42)
        );
        assert_eq!(
            Color::Rgb(1, 2, 3).into_termina(),
            ColorSpec::TrueColor(RgbColor::new(1, 2, 3).into())
        );
    }

    #[test]
    fn converts_modifier_diffs_to_sgr_modifiers() {
        let from = Modifier::BOLD | Modifier::ITALIC | Modifier::UNDERLINED;
        let to = Modifier::DIM | Modifier::REVERSED | Modifier::CROSSED_OUT;
        let modifiers = ModifierDiff { from, to }.into_termina();

        assert!(modifiers.contains(SgrModifiers::INTENSITY_NORMAL));
        assert!(modifiers.contains(SgrModifiers::NO_ITALIC));
        assert!(modifiers.contains(SgrModifiers::UNDERLINE_NONE));
        assert!(modifiers.contains(SgrModifiers::INTENSITY_DIM));
        assert!(modifiers.contains(SgrModifiers::REVERSE));
        assert!(modifiers.contains(SgrModifiers::STRIKE_THROUGH));
    }

    #[test]
    fn csi_helpers_use_one_based_coordinates() {
        assert_eq!(
            cursor_position(Position::new(1, 2)).unwrap(),
            Cursor::Position {
                line: OneBased::new(3).unwrap(),
                col: OneBased::new(2).unwrap(),
            }
        );
        assert_eq!(one_based(0).unwrap(), OneBased::new(1).unwrap());
    }
}
