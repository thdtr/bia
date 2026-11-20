//! Draws a [`Terminal`]'s screen and sends it keys, the mouse wheel,
//! selections and pastes. Runs of cells with the same look become one
//! cached `Paragraph`, placed at their column so the grid stays aligned.
//!
//! Holding Ctrl (Cmd on a Mac) underlines a file location under the mouse,
//! like `src/main.rs:42:7`, and a click opens it in the editor.

use crate::fonts;
use crate::frame_cache::FrameCache;
use crate::terminal::{Size, Terminal, key_bytes};
use crate::ui::style::Colors;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point as GridPoint, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::TermMode;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::vte::ansi::{Color as TermColor, CursorShape, NamedColor, Rgb};
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad};
use iced::advanced::text::{self, Paragraph as _, Renderer as _};
use iced::advanced::widget::{self, Widget, operation, tree};
use iced::advanced::{Clipboard, Renderer as _, Shell, clipboard, mouse};
use iced::alignment::Vertical;
use iced::keyboard::key::Named;
use iced::widget::text::{LineHeight, Shaping, Wrapping};
use iced::{
    Background, Border, Color, Element, Event, Font, Length, Pixels, Point, Rectangle, Shadow,
    Size as UiSize, Theme, font, keyboard, window,
};
use std::cell::RefCell;
use std::hash::{Hash, Hasher};

type Renderer = iced::Renderer;
type Paragraph = <Renderer as text::Renderer>::Paragraph;

const PAD: f32 = 8.0;
const LINE_HEIGHT: f32 = 1.3;

pub struct TerminalView<'a, Message> {
    id: widget::Id,
    terminal: &'a Terminal,
    font: Font,
    size: f32,
    colors: Colors,
    on_resize: Box<dyn Fn(Size) -> Message + 'a>,
    on_open: Box<dyn Fn(String) -> Message + 'a>,
}

impl<'a, Message> TerminalView<'a, Message> {
    pub fn new(
        id: &'static str,
        terminal: &'a Terminal,
        font: Font,
        size: f32,
        colors: Colors,
        on_resize: impl Fn(Size) -> Message + 'a,
        on_open: impl Fn(String) -> Message + 'a,
    ) -> Self {
        Self {
            id: widget::Id::new(id),
            terminal,
            font,
            size,
            colors,
            on_resize: Box::new(on_resize),
            on_open: Box::new(on_open),
        }
    }

    /// The file location under `point`: its screen row, its columns and
    /// the location itself.
    fn link_at(
        &self,
        point: Point,
        cell: (f32, f32),
    ) -> Option<(usize, std::ops::Range<usize>, String)> {
        let term = self.terminal.term.lock();
        let offset = term.grid().display_offset();
        let (at, _) = self.grid_point(point, cell, offset);
        let row = &term.grid()[at.line];
        let chars: Vec<char> = (0..term.columns()).map(|c| row[Column(c)].c).collect();
        let (columns, location) = bia_core::text::location_at(&chars, at.column.0)?;
        let screen_row = (at.line.0 + offset as i32) as usize;
        Some((screen_row, columns, location))
    }

    fn cell(&self, cache: &mut Cache) -> (f32, f32) {
        let key = (self.font, self.size.to_bits());
        if cache.cell.0 != Some(key) {
            let w = fonts::char_width(self.font, self.size, LINE_HEIGHT);
            cache.cell = (Some(key), w);
        }
        (cache.cell.1, (self.size * LINE_HEIGHT).round())
    }

    /// The grid cell under `point` (relative to the widget), and which half.
    fn grid_point(
        &self,
        point: Point,
        cell: (f32, f32),
        display_offset: usize,
    ) -> (GridPoint, Side) {
        let size = self.terminal.size;
        let col = ((point.x - PAD) / cell.0).max(0.0);
        let row = ((point.y - PAD) / cell.1).floor().clamp(0.0, size.lines.saturating_sub(1) as f32)
            as i32;
        let column = (col.floor() as usize).min(size.columns.saturating_sub(1));
        let side = if col.fract() < 0.5 { Side::Left } else { Side::Right };
        (GridPoint::new(Line(row - display_offset as i32), Column(column)), side)
    }
}

#[derive(Default)]
struct Cache {
    paragraphs: FrameCache<Paragraph>,
    cell: (Option<(Font, u32)>, f32),
}

#[derive(Default)]
struct State {
    focused: bool,
    selecting: bool,
    modifiers: keyboard::Modifiers,
    /// The location under the mouse while Ctrl is held.
    link: Option<(usize, std::ops::Range<usize>, String)>,
    cache: RefCell<Cache>,
}

impl operation::Focusable for State {
    fn is_focused(&self) -> bool {
        self.focused
    }

    fn focus(&mut self) {
        self.focused = true;
    }

    fn unfocus(&mut self) {
        self.focused = false;
    }
}

/// Keys that belong to BIA even with the terminal focused: open a file,
/// switch tabs, hide the terminal, and Ctrl+Shift shortcuts other than
/// copy and paste.
fn is_app_shortcut(
    key: &keyboard::Key,
    physical: keyboard::key::Physical,
    mods: keyboard::Modifiers,
) -> bool {
    // Ctrl+` hides the terminal and Ctrl+Shift+` opens another, on every
    // system (see `keys::shortcut`).
    if mods.control() && crate::keys::is_backquote(key, physical) {
        return true;
    }
    if !mods.command() {
        return false;
    }
    if matches!(key, keyboard::Key::Named(Named::Tab)) {
        return true;
    }
    let Some(c) = key.to_latin(physical) else { return false };
    let c = c.to_ascii_lowercase();
    if mods.shift() {
        return c != 'c' && c != 'v';
    }
    matches!(c, 'p' | '`' | ',' | '=' | '+' | '-' | '0' | 'b' | 'q')
}

impl<Message> Widget<Message, Theme, Renderer> for TerminalView<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn size(&self) -> UiSize<Length> {
        UiSize::new(Length::Fill, Length::Fill)
    }

    fn layout(
        &mut self,
        _tree: &mut widget::Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::Node::new(limits.max())
    }

    fn operate(
        &mut self,
        tree: &mut widget::Tree,
        layout: Layout<'_>,
        _renderer: &Renderer,
        operation: &mut dyn operation::Operation,
    ) {
        operation.focusable(Some(&self.id), layout.bounds(), tree.state.downcast_mut::<State>());
    }

    fn update(
        &mut self,
        tree: &mut widget::Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();
        let bounds = layout.bounds();
        let cell = self.cell(&mut state.cache.borrow_mut());
        if let Event::Keyboard(keyboard::Event::ModifiersChanged(m)) = event {
            state.modifiers = *m;
        }
        if matches!(
            event,
            Event::Mouse(mouse::Event::CursorMoved { .. })
                | Event::Keyboard(keyboard::Event::ModifiersChanged(_))
        ) {
            let link = cursor
                .position_in(bounds)
                .filter(|_| state.modifiers.command() && !state.selecting)
                .and_then(|point| self.link_at(point, cell));
            if link.as_ref().map(|l| (l.0, &l.1)) != state.link.as_ref().map(|l| (l.0, &l.1)) {
                state.link = link;
                shell.request_redraw();
            }
        }
        match event {
            Event::Window(window::Event::RedrawRequested(_)) => {
                // Fit the grid to the panel.
                let size = Size {
                    columns: ((bounds.width - 2.0 * PAD) / cell.0).floor().max(2.0) as usize,
                    lines: ((bounds.height - 2.0 * PAD) / cell.1).floor().max(1.0) as usize,
                };
                if size != self.terminal.size {
                    shell.publish((self.on_resize)(size));
                }
            }
            Event::Keyboard(keyboard::Event::KeyPressed {
                key,
                physical_key,
                modifiers,
                text,
                ..
            }) if state.focused => {
                if is_app_shortcut(key, *physical_key, *modifiers) {
                    return;
                }
                let latin = key.to_latin(*physical_key).map(|c| c.to_ascii_lowercase());
                if modifiers.command() && modifiers.shift() && latin == Some('c') {
                    let text = self.terminal.term.lock().selection_to_string();
                    if let Some(text) = text.filter(|t| !t.is_empty()) {
                        clipboard.write(clipboard::Kind::Standard, text);
                    }
                    shell.capture_event();
                    return;
                }
                if modifiers.command() && modifiers.shift() && latin == Some('v') {
                    if let Some(text) = clipboard.read(clipboard::Kind::Standard) {
                        self.terminal.paste(&text);
                    }
                    shell.capture_event();
                    return;
                }
                let bytes = key_bytes(key, *modifiers, self.terminal.app_cursor()).or_else(|| {
                    let text = text.as_ref()?;
                    let mut bytes = Vec::new();
                    if modifiers.alt() && !modifiers.control() {
                        bytes.push(0x1b);
                    }
                    bytes.extend(text.as_bytes());
                    (!text.is_empty()).then_some(bytes)
                });
                if let Some(bytes) = bytes {
                    self.terminal.term.lock().selection = None;
                    self.terminal.input(bytes);
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let Some(point) = cursor.position_in(bounds) else {
                    state.focused = false;
                    return;
                };
                state.focused = true;
                if state.modifiers.command()
                    && let Some((.., location)) = self.link_at(point, cell)
                {
                    state.link = None;
                    shell.publish((self.on_open)(location));
                    shell.capture_event();
                    return;
                }
                state.selecting = true;
                let mut term = self.terminal.term.lock();
                let (at, side) = self.grid_point(point, cell, term.grid().display_offset());
                term.selection = Some(Selection::new(SelectionType::Simple, at, side));
                shell.capture_event();
                shell.request_redraw();
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) if state.selecting => {
                let point = Point::new(position.x - bounds.x, position.y - bounds.y);
                let mut term = self.terminal.term.lock();
                let (at, side) = self.grid_point(point, cell, term.grid().display_offset());
                if let Some(selection) = &mut term.selection {
                    selection.update(at, side);
                }
                shell.request_redraw();
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if state.selecting {
                    state.selecting = false;
                    let mut term = self.terminal.term.lock();
                    // A click without a drag selects nothing.
                    if term.selection.as_ref().is_some_and(|s| s.is_empty()) {
                        term.selection = None;
                    }
                    shell.request_redraw();
                }
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) if cursor.is_over(bounds) => {
                let lines = match *delta {
                    mouse::ScrollDelta::Lines { y, .. } => (y * 3.0).round() as i32,
                    mouse::ScrollDelta::Pixels { y, .. } => (y / cell.1).round() as i32,
                };
                if lines != 0 {
                    let alt_screen =
                        self.terminal.term.lock().mode().contains(TermMode::ALT_SCREEN);
                    if alt_screen {
                        // Full-screen programs (less, vim) scroll with arrows.
                        let key = if lines > 0 { "A" } else { "B" };
                        let arrow = if self.terminal.app_cursor() {
                            format!("\x1bO{key}")
                        } else {
                            format!("\x1b[{key}")
                        };
                        self.terminal
                            .write(arrow.repeat(lines.unsigned_abs() as usize).into_bytes());
                    } else {
                        self.terminal.scroll(lines);
                    }
                    shell.request_redraw();
                }
                shell.capture_event();
            }
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        tree: &widget::Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        if tree.state.downcast_ref::<State>().link.is_some() && cursor.is_over(layout.bounds()) {
            mouse::Interaction::Pointer
        } else if cursor.is_over(layout.bounds()) {
            mouse::Interaction::Text
        } else {
            mouse::Interaction::None
        }
    }

    fn draw(
        &self,
        tree: &widget::Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();
        let bounds = layout.bounds();
        let mut cache = state.cache.borrow_mut();
        let (cw, ch) = self.cell(&mut cache);
        let c = self.colors;
        let palette = Palette::for_colors(c);
        cache.paragraphs.next_frame();

        let term = self.terminal.term.lock();
        let content = term.renderable_content();
        let offset = content.display_offset as i32;
        let lines = term.screen_lines();
        let origin = Point::new(bounds.x + PAD, bounds.y + PAD);
        let at = |row: usize, col: usize| {
            Point::new(origin.x + col as f32 * cw, origin.y + row as f32 * ch)
        };

        let mut backgrounds: Vec<(Rectangle, Color)> = Vec::new();
        let mut runs: Vec<(Point, Paragraph, Color)> = Vec::new();
        // The run being built: row, first column, text, color, bold, italic.
        let mut run: Option<(usize, usize, String, Color, bool, bool)> = None;
        let mut last_col = usize::MAX;
        let flush = |run: &mut Option<(usize, usize, String, Color, bool, bool)>,
                     cache: &mut Cache,
                     runs: &mut Vec<(Point, Paragraph, Color)>| {
            if let Some((row, col, text, color, bold, italic)) = run.take()
                && !text.trim().is_empty()
            {
                let p = paragraph(cache, &text, self.font, self.size, bold, italic);
                runs.push((at(row, col), p, color));
            }
        };
        for indexed in content.display_iter {
            let row = indexed.point.line.0 + offset;
            if row < 0 || row as usize >= lines {
                continue;
            }
            let (row, col) = (row as usize, indexed.point.column.0);
            let flags = indexed.cell.flags;
            if flags.contains(Flags::WIDE_CHAR_SPACER)
                || flags.contains(Flags::LEADING_WIDE_CHAR_SPACER)
            {
                continue;
            }
            let mut fg = palette.resolve(indexed.cell.fg, content.colors, true);
            let mut bg = palette.resolve(indexed.cell.bg, content.colors, false);
            if flags.contains(Flags::INVERSE) {
                std::mem::swap(&mut fg, &mut bg);
            }
            if flags.contains(Flags::DIM) {
                fg.a *= 0.66;
            }
            if flags.contains(Flags::HIDDEN) {
                fg = bg;
            }
            let wide = flags.contains(Flags::WIDE_CHAR);
            let width = if wide { 2 } else { 1 };
            let selected = content.selection.is_some_and(|s| s.contains(indexed.point));
            if selected {
                bg = c.selection;
            }
            if bg != palette.background || selected {
                backgrounds
                    .push((Rectangle::new(at(row, col), UiSize::new(cw * width as f32, ch)), bg));
            }
            let bold = flags.contains(Flags::BOLD);
            let italic = flags.contains(Flags::ITALIC);
            let continues = matches!(&run, Some((r, _, _, color, b, i)) if *r == row && *color == fg && *b == bold && *i == italic)
                && last_col.wrapping_add(1) == col
                && !wide;
            if !continues {
                flush(&mut run, &mut cache, &mut runs);
                run = Some((row, col, String::new(), fg, bold, italic));
            }
            if let Some((_, _, text, ..)) = &mut run {
                text.push(indexed.cell.c);
            }
            last_col = if wide { usize::MAX - 1 } else { col };
            if wide {
                flush(&mut run, &mut cache, &mut runs);
            }
        }
        flush(&mut run, &mut cache, &mut runs);

        let cursor = content.cursor;
        let cursor_row = cursor.point.line.0 + offset;
        drop(term);
        drop(cache);

        renderer.with_layer(bounds, |renderer| {
            let fill = |renderer: &mut Renderer, rect: Rectangle, color: Color| {
                renderer.fill_quad(
                    Quad {
                        bounds: rect,
                        border: Border::default(),
                        shadow: Shadow::default(),
                        snap: true,
                    },
                    Background::Color(color),
                );
            };
            fill(renderer, bounds, palette.background);
            for (rect, color) in &backgrounds {
                fill(renderer, *rect, *color);
            }
            for (point, paragraph, color) in &runs {
                renderer.fill_paragraph(paragraph, *point, *color, bounds);
            }
            if let Some((row, columns, _)) = &state.link {
                let start = at(*row, columns.start);
                let width = (columns.end - columns.start) as f32 * cw;
                let line = Rectangle::new(
                    Point::new(start.x, start.y + ch - 2.0),
                    UiSize::new(width, 1.0),
                );
                fill(renderer, line, c.accent);
            }
        });
        // The cursor, over the text.
        if cursor.shape != CursorShape::Hidden && cursor_row >= 0 && (cursor_row as usize) < lines {
            renderer.with_layer(bounds, |renderer| {
                let p = at(cursor_row as usize, cursor.point.column.0);
                let (rect, border) = match (cursor.shape, state.focused) {
                    (_, false) | (CursorShape::HollowBlock, _) => (
                        Rectangle::new(p, UiSize::new(cw, ch)),
                        Border { color: c.accent, width: 1.0, radius: 0.0.into() },
                    ),
                    (CursorShape::Beam, _) => {
                        (Rectangle::new(p, UiSize::new(2.0, ch)), Border::default())
                    }
                    (CursorShape::Underline, _) => (
                        Rectangle::new(Point::new(p.x, p.y + ch - 2.0), UiSize::new(cw, 2.0)),
                        Border::default(),
                    ),
                    _ => (Rectangle::new(p, UiSize::new(cw, ch)), Border::default()),
                };
                let fill = if border.width > 0.0 {
                    Color::TRANSPARENT
                } else {
                    Color { a: 0.6, ..c.accent }
                };
                renderer.fill_quad(
                    Quad { bounds: rect, border, shadow: Shadow::default(), snap: true },
                    Background::Color(fill),
                );
            });
        }
    }
}

fn paragraph(
    cache: &mut Cache,
    content: &str,
    base: Font,
    size: f32,
    bold: bool,
    italic: bool,
) -> Paragraph {
    let mut hasher = std::hash::DefaultHasher::new();
    (content, base, size.to_bits(), bold, italic).hash(&mut hasher);
    let key = hasher.finish();
    cache.paragraphs.get_or_insert_with(key, || styled(content, base, size, bold, italic, key))
}

fn styled(content: &str, base: Font, size: f32, bold: bool, italic: bool, key: u64) -> Paragraph {
    let mut font = base;
    if bold {
        font.weight = font::Weight::Bold;
    }
    if italic {
        font.style = font::Style::Italic;
    }
    Paragraph::with_text(text::Text {
        content,
        bounds: fonts::tagged_bounds(f32::INFINITY, key),
        size: Pixels(size),
        line_height: LineHeight::Relative(LINE_HEIGHT),
        font,
        align_x: text::Alignment::Default,
        align_y: Vertical::Top,
        shaping: Shaping::Advanced,
        wrapping: Wrapping::None,
    })
}

/// The 16 ANSI colors, picked to sit well on the theme's background.
struct Palette {
    foreground: Color,
    background: Color,
    ansi: [Color; 16],
}

impl Palette {
    fn for_colors(c: Colors) -> Self {
        let b = c.background;
        let dark = 0.299 * b.r + 0.587 * b.g + 0.114 * b.b < 0.5;
        let hex = |v: u32| Color::from_rgb8((v >> 16) as u8, (v >> 8) as u8, v as u8);
        let ansi = if dark {
            [
                0x2a2830, 0xe5736b, 0x9cc48a, 0xebc77f, 0x7aa2e3, 0xd6a3cf, 0x6fc4b8, 0xcfc9be,
                0x77717f, 0xf0918a, 0xb4d6a4, 0xf2d69e, 0x9bbaf0, 0xe4bfde, 0x93d8ce, 0xf2ede4,
            ]
        } else {
            [
                0x2b2824, 0xb3261e, 0x4f8a2e, 0x9a6a00, 0x2f5fb3, 0x8e3f86, 0x1f7f72, 0x8b8478,
                0x5c564d, 0xd0453b, 0x63a33d, 0xb98318, 0x4b78c9, 0xa65aa0, 0x2f9a8b, 0x2b2824,
            ]
        }
        .map(hex);
        Self { foreground: c.text, background: c.background, ansi }
    }

    fn resolve(
        &self,
        color: TermColor,
        overrides: &alacritty_terminal::term::color::Colors,
        foreground: bool,
    ) -> Color {
        let rgb = |rgb: Rgb| Color::from_rgb8(rgb.r, rgb.g, rgb.b);
        match color {
            TermColor::Spec(spec) => rgb(spec),
            TermColor::Named(named) => {
                if let Some(o) = overrides[named] {
                    return rgb(o);
                }
                match named {
                    NamedColor::Foreground | NamedColor::BrightForeground => self.foreground,
                    NamedColor::Background => self.background,
                    NamedColor::DimForeground => Color { a: 0.66, ..self.foreground },
                    NamedColor::Cursor => self.foreground,
                    n => {
                        let i = n as usize;
                        if i < 16 {
                            self.ansi[i]
                        } else if (NamedColor::DimBlack as usize..=NamedColor::DimWhite as usize)
                            .contains(&i)
                        {
                            Color { a: 0.66, ..self.ansi[i - NamedColor::DimBlack as usize] }
                        } else if foreground {
                            self.foreground
                        } else {
                            self.background
                        }
                    }
                }
            }
            TermColor::Indexed(i) => {
                if let Some(o) = overrides[i as usize] {
                    return rgb(o);
                }
                match i {
                    0..=15 => self.ansi[i as usize],
                    16..=231 => {
                        let i = i - 16;
                        let level = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
                        Color::from_rgb8(level(i / 36), level((i / 6) % 6), level(i % 6))
                    }
                    _ => {
                        let v = 8 + (i - 232) * 10;
                        Color::from_rgb8(v, v, v)
                    }
                }
            }
        }
    }
}

impl<'a, Message: 'a> From<TerminalView<'a, Message>> for Element<'a, Message, Theme, Renderer> {
    fn from(view: TerminalView<'a, Message>) -> Self {
        Element::new(view)
    }
}
