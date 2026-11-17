//! The code editor: draws an [`Editor`] and turns keys and the mouse into
//! [`Input`] messages. It owns nothing but how the view looks right now
//! (scroll, focus, the mouse drag); the text and cursors live in the app.
//!
//! Only the rows on screen are laid out, one `Paragraph` per row, cached by
//! content. Selections, the current line, search matches and the bracket
//! pair are rectangles under the text; cursors go over it.

mod row;

use crate::buffer::GitMarks;
use crate::fonts;
use crate::keys::{self, Command};
use crate::syntax::{self, Highlights, LineStyle};
use crate::ui::style::Colors;
use bia_core::editor::{DisplayMap, Editor, Motion, Selection, Unit};
use bia_core::git::Mark;
use iced::advanced::Renderer as _;
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad};
use iced::advanced::text::{self, Paragraph as _, Renderer as _};
use iced::advanced::widget::{self, Widget, operation, tree};
use iced::advanced::{Clipboard, InputMethod, Shell, clipboard, input_method, mouse};
use iced::alignment::Vertical;
use iced::widget::text::{LineHeight, Shaping, Span, Wrapping};
use iced::{
    Background, Border, Color, Element, Event, Font, Length, Pixels, Point, Rectangle, Shadow,
    Size, Theme, Vector, font, keyboard, window,
};
use row::RowText;
use std::cell::{Ref, RefCell};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::time::{Duration, Instant};
use unicode_segmentation::UnicodeSegmentation;

type Renderer = iced::Renderer;
type Paragraph = <Renderer as text::Renderer>::Paragraph;

const PAD_TOP: f32 = 10.0;
const GUTTER_LEFT: f32 = 14.0;
const GUTTER_RIGHT: f32 = 18.0;
const TEXT_LEFT: f32 = 4.0;
const SCROLLBAR: f32 = 10.0;
const BLINK: Duration = Duration::from_millis(500);
/// Paragraphs kept before the cache starts over.
const MAX_CACHED: usize = 3_000;

/// What the editor asks the app to do.
#[derive(Debug, Clone)]
pub enum Input {
    Click {
        at: usize,
        extend: bool,
    },
    /// Alt+click: add or remove a cursor.
    ToggleCursor(usize),
    /// Double or triple click: a word or a line.
    Select(Range<usize>),
    /// The mouse moved with the button down, from what was first clicked.
    Drag {
        origin: Range<usize>,
        at: usize,
        unit: Unit,
    },
    Command(Command),
    /// Typed text, or what an input method composed.
    Text(String),
    Paste(String),
}

pub struct CodeView<'a, Message> {
    id: widget::Id,
    editor: &'a Editor,
    highlights: &'a RefCell<Highlights>,
    display: &'a RefCell<DisplayMap>,
    syntax: syntax::Context<'a>,
    matches: &'a [Range<usize>],
    current_match: Option<usize>,
    git: Option<&'a RefCell<GitMarks>>,
    font: Font,
    size: f32,
    line_height: f32,
    colors: Colors,
    wrap: bool,
    /// Which file this is, so switching tabs restores its own scroll.
    key: u64,
    scroll: Vector,
    /// Changes when the app wants the cursor centered (go to line).
    center: u64,
    on_input: Box<dyn Fn(Input) -> Message + 'a>,
    on_scroll: Box<dyn Fn(Vector) -> Message + 'a>,
}

impl<'a, Message> CodeView<'a, Message> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: &'static str,
        editor: &'a Editor,
        highlights: &'a RefCell<Highlights>,
        display: &'a RefCell<DisplayMap>,
        syntax: syntax::Context<'a>,
        colors: Colors,
        on_input: impl Fn(Input) -> Message + 'a,
        on_scroll: impl Fn(Vector) -> Message + 'a,
    ) -> Self {
        Self {
            id: widget::Id::new(id),
            editor,
            highlights,
            display,
            syntax,
            matches: &[],
            current_match: None,
            git: None,
            font: Font::MONOSPACE,
            size: 14.0,
            line_height: 1.5,
            colors,
            wrap: false,
            key: 0,
            scroll: Vector::ZERO,
            center: 0,
            on_input: Box::new(on_input),
            on_scroll: Box::new(on_scroll),
        }
    }

    pub fn font(mut self, font: Font, size: f32, line_height: f32) -> Self {
        (self.font, self.size, self.line_height) = (font, size, line_height);
        self
    }

    /// Search matches to light up, and which one is current.
    pub fn matches(mut self, matches: &'a [Range<usize>], current: Option<usize>) -> Self {
        (self.matches, self.current_match) = (matches, current);
        self
    }

    /// Lines changed since the last commit, shown in the gutter.
    pub fn git(mut self, git: &'a RefCell<GitMarks>) -> Self {
        self.git = Some(git);
        self
    }

    pub fn wrap(mut self, wrap: bool) -> Self {
        self.wrap = wrap;
        self
    }

    /// `key` identifies the file; `scroll` is where to start when it changes.
    pub fn scroll(mut self, key: u64, scroll: Vector, center: u64) -> Self {
        (self.key, self.scroll, self.center) = (key, scroll, center);
        self
    }
}

// ── State ────────────────────────────────────────────────────────────────

struct Focus {
    since: Instant,
    now: Instant,
    window: bool,
}

impl Focus {
    fn new() -> Self {
        let now = Instant::now();
        Self { since: now, now, window: true }
    }

    fn cursor_visible(&self) -> bool {
        self.window && ((self.now - self.since).as_millis() / BLINK.as_millis()).is_multiple_of(2)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Bar {
    Vertical { grab: f32 },
    Horizontal { grab: f32 },
}

#[derive(Default)]
struct Cache {
    paragraphs: HashMap<u64, Paragraph>,
    char_width: Option<((Font, u32, u32), f32)>,
    /// The widest line in cells, by editor revision and file.
    widest: Option<((u64, u64), usize)>,
}

#[derive(Default)]
struct State {
    focus: Option<Focus>,
    offset: Vector,
    key: Option<u64>,
    center: u64,
    /// The cursor last scrolled into view: file, revision, head, count.
    followed: (u64, u64, usize, usize),
    drag: Option<(Range<usize>, Unit)>,
    bar: Option<Bar>,
    last_click: Option<mouse::Click>,
    modifiers: keyboard::Modifiers,
    preedit: Option<input_method::Preedit>,
    cache: RefCell<Cache>,
}

impl operation::Focusable for State {
    fn is_focused(&self) -> bool {
        self.focus.is_some()
    }

    fn focus(&mut self) {
        self.focus = Some(Focus::new());
    }

    fn unfocus(&mut self) {
        self.focus = None;
    }
}

/// Sizes that follow from the font and the widget's bounds.
#[derive(Clone, Copy)]
struct Metrics {
    line: f32,
    char: f32,
    gutter: f32,
    /// Left edge of the text, relative to the widget.
    text_x: f32,
    width: f32,
    height: f32,
}

impl Metrics {
    fn text_width(&self) -> f32 {
        (self.width - self.text_x).max(0.0)
    }

    fn visible_rows(&self) -> usize {
        (self.height / self.line).floor().max(1.0) as usize
    }
}

/// One row on screen: line `line`, bytes `start..end` of it.
struct Row {
    line: usize,
    start: usize,
    end: usize,
    /// The line's last row (where its `\n` is).
    last: bool,
}

// ── Geometry ─────────────────────────────────────────────────────────────

impl<Message> CodeView<'_, Message> {
    fn metrics(&self, bounds: Rectangle, cache: &mut Cache) -> Metrics {
        let key = (self.font, self.size.to_bits(), self.line_height.to_bits());
        let char = match cache.char_width {
            Some((k, w)) if k == key => w,
            _ => {
                let w = fonts::char_width(self.font, self.size, self.line_height);
                cache.char_width = Some((key, w));
                w
            }
        };
        let digits = self.editor.line_count().to_string().len().max(2) as f32;
        let gutter = GUTTER_LEFT + digits * char + GUTTER_RIGHT;
        Metrics {
            line: (self.size * self.line_height).round(),
            char,
            gutter,
            text_x: gutter + TEXT_LEFT,
            width: bounds.width,
            height: bounds.height,
        }
    }

    /// The file's rows, brought up to date first if the text or the width
    /// changed. The map lives in the buffer, where Up and Down use it too.
    fn display(&self, m: &Metrics) -> Ref<'_, DisplayMap> {
        let wrap =
            self.wrap.then(|| ((m.text_width() - SCROLLBAR) / m.char).floor().max(8.0) as usize);
        if !self.display.borrow().is_current(self.editor, wrap) {
            self.display.borrow_mut().sync(self.editor, wrap);
        }
        self.display.borrow()
    }

    /// The widest line, in cells. Measured over the whole file once; after
    /// edits, only the changed lines are measured, so it never shrinks (a
    /// little extra room to scroll sideways is harmless).
    fn widest(&self, cache: &mut Cache) -> usize {
        let tab = self.editor.tab_width;
        let width = |l: ropey::RopeSlice<'_>| {
            l.chars()
                .filter(|&c| c != '\n')
                .fold(0, |col, c| col + bia_core::editor::char_width(c, col, tab))
        };
        let rope = self.editor.rope();
        let revision = self.editor.revision();
        match cache.widest {
            Some(((key, rev), w)) if key == self.key && rev == revision => w,
            Some(((key, rev), w)) if key == self.key => {
                let from = self.editor.first_changed_line(rev).unwrap_or(0);
                let to = (from + 2_000).min(rope.len_lines());
                let w = (from..to).map(|l| width(rope.line(l))).max().unwrap_or(0).max(w);
                cache.widest = Some(((self.key, revision), w));
                w
            }
            _ => {
                let w = rope.lines().map(width).max().unwrap_or(0);
                cache.widest = Some(((self.key, revision), w));
                w
            }
        }
    }

    /// How far the view can scroll.
    fn max_offset(&self, m: &Metrics, cache: &mut Cache) -> Vector {
        let rows = self.display(m).row_count() as f32;
        let content = PAD_TOP + rows * m.line;
        // A file that fits doesn't scroll at all; a longer one can scroll
        // until its last line is halfway up.
        let y = if content + m.line <= m.height { 0.0 } else { content - m.height / 2.0 };
        let x = if self.wrap {
            0.0
        } else {
            let wide = (self.widest(cache) as f32 + 2.0) * m.char;
            (wide - m.text_width()).max(0.0)
        };
        Vector::new(x.max(0.0), y.max(0.0))
    }

    fn row(&self, display: &DisplayMap, row: usize, line_text: &str) -> Row {
        let line = display.line_of_row(row);
        let starts = display.row_starts(line_text);
        let k = (row - display.first_row(line)).min(starts.len() - 1);
        let end = starts.get(k + 1).copied().unwrap_or(line_text.len());
        Row { line, start: starts[k], end, last: k + 1 == starts.len() }
    }

    /// The text offset under `point` (relative to the widget).
    fn hit(&self, point: Point, m: &Metrics, state: &State) -> usize {
        let display = self.display(m);
        let y = point.y - PAD_TOP + state.offset.y;
        let row = (y / m.line).floor().max(0.0) as usize;
        let row = row.min(display.row_count().saturating_sub(1));
        let line = display.line_of_row(row);
        let text = self.editor.line(line);
        let r = self.row(&display, row, &text);
        let row_text = RowText::new(&text, r.start, r.end, self.editor.tab_width);
        let x = point.x - m.text_x + state.offset.x;
        let column = if x <= 0.0 {
            r.start
        } else {
            let paragraph = self.paragraph(&row_text.text, &[]);
            match paragraph.hit_test(Point::new(x, m.line / 2.0)) {
                Some(hit) => row_text.to_line(hit.cursor()),
                None => r.end,
            }
        };
        // Past the end of a wrapped row, stay on it rather than jump to
        // the start of the next.
        let column = if !r.last && column >= r.end && r.end > r.start {
            text[..r.end].char_indices().next_back().map_or(r.start, |(i, _)| i)
        } else {
            column
        };
        self.editor.line_start(line) + column
    }

    /// Where the cursor at `at` is drawn, relative to the content's origin
    /// (before scrolling): its row's top and its x.
    fn caret(&self, at: usize, m: &Metrics) -> (usize, f32) {
        let (line, col) = self.editor.line_col(at);
        let text = self.editor.line(line);
        let display = self.display(m);
        let row = display.first_row(line) + display.row_in_line(&text, col);
        let r = self.row(&display, row, &text);
        let row_text = RowText::new(&text, r.start, r.end, self.editor.tab_width);
        let index = row_text.text[..row_text.to_text(col)].graphemes(true).count();
        let paragraph = self.paragraph(&row_text.text, &[]);
        let x = paragraph.grapheme_position(0, index).map_or(0.0, |p| p.x);
        (row, x)
    }

    fn paragraph(&self, content: &str, spans: &[Span<'_, (), Font>]) -> Paragraph {
        fn text<T>(view: &CodeView<'_, impl Sized>, content: T) -> text::Text<T, Font> {
            text::Text {
                content,
                bounds: Size::INFINITE,
                size: Pixels(view.size),
                line_height: LineHeight::Relative(view.line_height),
                font: view.font,
                align_x: text::Alignment::Default,
                align_y: Vertical::Top,
                shaping: Shaping::Advanced,
                wrapping: Wrapping::None,
            }
        }
        if spans.is_empty() {
            Paragraph::with_text(text(self, content))
        } else {
            Paragraph::with_spans(text(self, spans))
        }
    }
}

// ── Scrolling ────────────────────────────────────────────────────────────

impl<Message> CodeView<'_, Message> {
    /// Puts the scroll back in range and follows the cursor when it moved.
    /// Returns whether the offset changed.
    fn sync_scroll(&self, state: &mut State, m: &Metrics) -> bool {
        let before = state.offset;
        let primary = self.editor.selections().primary();
        let marker =
            (self.key, self.editor.revision(), primary.head, self.editor.selections().len());
        if state.key != Some(self.key) {
            state.key = Some(self.key);
            state.offset = self.scroll;
            state.followed = marker;
            state.drag = None;
            if state.center == self.center {
                return false;
            }
        }
        let max = self.max_offset(m, &mut state.cache.borrow_mut());
        if state.center != self.center {
            state.center = self.center;
            state.followed = marker;
            let (row, x) = self.caret(primary.head, m);
            state.offset.y = PAD_TOP + row as f32 * m.line - m.height / 2.0 + m.line / 2.0;
            state.offset.x = (x - m.text_width() + 4.0 * m.char).max(0.0);
        } else if state.followed != marker {
            state.followed = marker;
            let (row, x) = self.caret(primary.head, m);
            let top = PAD_TOP + row as f32 * m.line;
            let margin = m.line;
            if top - margin < state.offset.y {
                state.offset.y = top - margin;
            } else if top + m.line + margin > state.offset.y + m.height {
                state.offset.y = top + m.line + margin - m.height;
            }
            let room = m.text_width() - SCROLLBAR;
            if x - 2.0 * m.char < state.offset.x {
                state.offset.x = x - 2.0 * m.char;
            } else if x + 4.0 * m.char > state.offset.x + room {
                state.offset.x = x + 4.0 * m.char - room;
            }
        }
        state.offset.x = state.offset.x.clamp(0.0, max.x);
        state.offset.y = state.offset.y.clamp(0.0, max.y);
        state.offset != before
    }

    fn bars(
        &self,
        m: &Metrics,
        max: Vector,
        offset: Vector,
    ) -> [Option<(Rectangle, Rectangle)>; 2] {
        let vertical = (max.y > 0.0).then(|| {
            let track = Rectangle::new(
                Point::new(m.width - SCROLLBAR, 0.0),
                Size::new(SCROLLBAR, m.height),
            );
            let h = (m.height * m.height / (m.height + max.y)).max(24.0);
            let y = (m.height - h) * offset.y / max.y;
            (
                track,
                Rectangle::new(
                    Point::new(m.width - SCROLLBAR + 2.0, y),
                    Size::new(SCROLLBAR - 4.0, h),
                ),
            )
        });
        let horizontal = (max.x > 0.0).then(|| {
            let width = m.text_width() - SCROLLBAR;
            let track = Rectangle::new(
                Point::new(m.text_x, m.height - SCROLLBAR),
                Size::new(width, SCROLLBAR),
            );
            let w = (width * width / (width + max.x)).max(24.0);
            let x = m.text_x + (width - w) * offset.x / max.x;
            (
                track,
                Rectangle::new(
                    Point::new(x, m.height - SCROLLBAR + 2.0),
                    Size::new(w, SCROLLBAR - 4.0),
                ),
            )
        });
        [vertical, horizontal]
    }
}

// ── Widget ───────────────────────────────────────────────────────────────

impl<Message> Widget<Message, Theme, Renderer> for CodeView<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
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
        let state = tree.state.downcast_mut::<State>();
        operation.focusable(Some(&self.id), layout.bounds(), state);
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
        let m = self.metrics(bounds, &mut state.cache.borrow_mut());
        let publish = |shell: &mut Shell<'_, Message>, input: Input| {
            shell.publish((self.on_input)(input));
            shell.capture_event();
        };

        match event {
            Event::Window(window::Event::Unfocused) => {
                if let Some(focus) = &mut state.focus {
                    focus.window = false;
                }
            }
            Event::Window(window::Event::Focused) => {
                if let Some(focus) = &mut state.focus {
                    focus.window = true;
                    focus.since = Instant::now();
                    shell.request_redraw();
                }
            }
            Event::Window(window::Event::RedrawRequested(now)) => {
                if self.sync_scroll(state, &m) {
                    shell.publish((self.on_scroll)(state.offset));
                }
                // Highlighting a far-away part of a long file goes on over
                // several frames.
                if self.highlights.borrow().pending() {
                    shell.request_redraw();
                }
                if let Some(focus) = &mut state.focus
                    && focus.window
                {
                    focus.now = *now;
                    let phase = (focus.now - focus.since).as_millis() % BLINK.as_millis();
                    shell.request_redraw_at(*now + BLINK - Duration::from_millis(phase as u64));
                    let method = self.input_method(state, &m, bounds);
                    shell.request_input_method(&method);
                }
            }
            Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                state.modifiers = *modifiers;
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let Some(point) = cursor.position_in(bounds) else {
                    state.focus = None;
                    return;
                };
                state.focus = Some(Focus::new());
                let max = self.max_offset(&m, &mut state.cache.borrow_mut());
                let [vertical, horizontal] = self.bars(&m, max, state.offset);
                if let Some((track, thumb)) = vertical
                    && track.contains(point)
                {
                    let grab =
                        if thumb.contains(point) { point.y - thumb.y } else { thumb.height / 2.0 };
                    state.bar = Some(Bar::Vertical { grab });
                    self.drag_bar(state, &m, max, point, shell);
                    shell.capture_event();
                    return;
                }
                if let Some((track, thumb)) = horizontal
                    && track.contains(point)
                {
                    let grab =
                        if thumb.contains(point) { point.x - thumb.x } else { thumb.width / 2.0 };
                    state.bar = Some(Bar::Horizontal { grab });
                    self.drag_bar(state, &m, max, point, shell);
                    shell.capture_event();
                    return;
                }

                let click = mouse::Click::new(point, mouse::Button::Left, state.last_click);
                state.last_click = Some(click);
                let at = self.hit(point, &m, state);
                let extend = state.modifiers.shift();
                if point.x < m.gutter {
                    let line = self.editor.line_range(at);
                    state.drag = Some((line.clone(), Unit::Line));
                    publish(shell, Input::Select(line));
                    return;
                }
                match click.kind() {
                    mouse::click::Kind::Single if state.modifiers.alt() => {
                        state.drag = None;
                        publish(shell, Input::ToggleCursor(at));
                    }
                    mouse::click::Kind::Single => {
                        let anchor = self.editor.selections().primary().anchor;
                        let origin = if extend { anchor..anchor } else { at..at };
                        state.drag = Some((origin, Unit::Char));
                        publish(shell, Input::Click { at, extend });
                    }
                    mouse::click::Kind::Double => {
                        let word = self.editor.word_range(at);
                        state.drag = Some((word.clone(), Unit::Word));
                        publish(shell, Input::Select(word));
                    }
                    mouse::click::Kind::Triple => {
                        let line = self.editor.line_range(at);
                        state.drag = Some((line.clone(), Unit::Line));
                        publish(shell, Input::Select(line));
                    }
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                let point = Point::new(position.x - bounds.x, position.y - bounds.y);
                if state.bar.is_some() {
                    let max = self.max_offset(&m, &mut state.cache.borrow_mut());
                    self.drag_bar(state, &m, max, point, shell);
                    shell.capture_event();
                } else if let Some((origin, unit)) = state.drag.clone() {
                    let at = self.hit(point, &m, state);
                    publish(shell, Input::Drag { origin, at, unit });
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                state.drag = None;
                state.bar = None;
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) if cursor.is_over(bounds) => {
                let (mut dx, mut dy) = match *delta {
                    mouse::ScrollDelta::Lines { x, y } => (-x * m.char * 6.0, -y * m.line * 3.0),
                    mouse::ScrollDelta::Pixels { x, y } => (-x, -y),
                };
                if state.modifiers.shift() && dx == 0.0 {
                    (dx, dy) = (dy, 0.0);
                }
                let max = self.max_offset(&m, &mut state.cache.borrow_mut());
                let next = Vector::new(
                    (state.offset.x + dx).clamp(0.0, max.x),
                    (state.offset.y + dy).clamp(0.0, max.y),
                );
                if next != state.offset {
                    state.offset = next;
                    shell.publish((self.on_scroll)(next));
                    shell.request_redraw();
                }
                shell.capture_event();
            }
            Event::Keyboard(keyboard::Event::KeyPressed {
                key,
                physical_key,
                modifiers,
                text,
                ..
            }) => {
                let Some(focus) = &mut state.focus else { return };
                focus.since = Instant::now();
                if let Some(command) = keys::editor_command(key, *physical_key, *modifiers) {
                    match command {
                        // Escape with one cursor belongs to the app (closing find…).
                        Command::Collapse if self.editor.selections().len() == 1 => {}
                        Command::Paste => {
                            if let Some(text) = clipboard.read(clipboard::Kind::Standard) {
                                publish(shell, Input::Paste(text));
                            }
                            shell.capture_event();
                        }
                        Command::Page { down, extend } => {
                            let rows = m.visible_rows().saturating_sub(2).max(1);
                            let motion =
                                if down { Motion::PageDown(rows) } else { Motion::PageUp(rows) };
                            publish(shell, Input::Command(Command::Move(motion, extend)));
                        }
                        command => publish(shell, Input::Command(command)),
                    }
                    return;
                }
                // Ctrl+Alt is AltGr on Windows, which types characters.
                let shortcut = modifiers.command() && !modifiers.alt();
                if let Some(text) = text
                    && !shortcut
                {
                    let typed: String = text.chars().filter(|c| !c.is_control()).collect();
                    if !typed.is_empty() {
                        publish(shell, Input::Text(typed));
                    }
                }
            }
            Event::InputMethod(event) if state.focus.is_some() => {
                match event {
                    input_method::Event::Opened => {
                        state.preedit = Some(input_method::Preedit::new())
                    }
                    input_method::Event::Closed => state.preedit = None,
                    input_method::Event::Preedit(content, selection) => {
                        state.preedit = Some(input_method::Preedit {
                            content: content.clone(),
                            selection: selection.clone(),
                            text_size: Some(Pixels(self.size)),
                        });
                    }
                    input_method::Event::Commit(text) => {
                        state.preedit = None;
                        shell.publish((self.on_input)(Input::Text(text.clone())));
                    }
                }
                shell.capture_event();
                shell.request_redraw();
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
        let state = tree.state.downcast_ref::<State>();
        let bounds = layout.bounds();
        let Some(point) = cursor.position_in(bounds) else { return mouse::Interaction::None };
        if state.bar.is_some() {
            return mouse::Interaction::Idle;
        }
        let m = self.metrics(bounds, &mut state.cache.borrow_mut());
        if point.x < m.gutter || point.x > m.width - SCROLLBAR {
            mouse::Interaction::Idle
        } else {
            mouse::Interaction::Text
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
        let m = self.metrics(bounds, &mut cache);
        let c = self.colors;
        let max = self.max_offset(&m, &mut cache);
        let offset =
            Vector::new(state.offset.x.clamp(0.0, max.x), state.offset.y.clamp(0.0, max.y));
        let display = self.display(&m);

        let first = ((offset.y - PAD_TOP) / m.line).floor().max(0.0) as usize;
        let last = (((offset.y - PAD_TOP + m.height) / m.line).ceil() as usize)
            .min(display.row_count().saturating_sub(1));
        let text_area = Rectangle::new(
            Point::new(bounds.x + m.gutter, bounds.y),
            Size::new((bounds.width - m.gutter).max(0.0), bounds.height),
        );
        let origin_x = bounds.x + m.text_x - offset.x;
        let row_y = |row: usize| bounds.y + PAD_TOP + row as f32 * m.line - offset.y;

        // What's on screen: rows, their text, and styles per line.
        let first_line = display.line_of_row(first);
        let last_line = display.line_of_row(last);
        let styles: Vec<LineStyle> =
            self.highlights.borrow_mut().lines(self.editor, self.syntax, first_line..last_line + 1);
        // iced keeps only weak references to paragraphs until it renders,
        // so everything drawn comes from (and stays in) the cache.
        if cache.paragraphs.len() > MAX_CACHED {
            cache.paragraphs.clear();
        }
        let mut rows: Vec<(usize, Row, RowText, Paragraph)> = Vec::new();
        let mut line_text = (usize::MAX, String::new());
        for row in first..=last {
            let line = display.line_of_row(row);
            if line_text.0 != line {
                line_text = (line, self.editor.line(line));
            }
            let r = self.row(&display, row, &line_text.1);
            let row_text = RowText::new(&line_text.1, r.start, r.end, self.editor.tab_width);
            let style = styles.get(line - first_line).cloned().unwrap_or_default();
            let paragraph = self.cached_paragraph(&mut cache, &row_text, &r, &style);
            rows.push((row, r, row_text, paragraph));
        }
        let current_line = self.editor.line_of(self.editor.selections().primary().head);
        let numbers: Vec<(usize, usize, Paragraph)> = rows
            .iter()
            .filter(|(_, r, ..)| r.start == 0)
            .map(|(row, r, ..)| (*row, r.line, self.number(&mut cache, r.line + 1)))
            .collect();
        drop(cache);

        let x_of = |row_text: &RowText, paragraph: &Paragraph, at_in_line: usize| {
            let index = row_text.text[..row_text.to_text(at_in_line)].graphemes(true).count();
            paragraph.grapheme_position(0, index).map_or(0.0, |p| p.x)
        };
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
        // A span of the text on one row, as a rectangle.
        let span_rect = |row: usize,
                         r: &Row,
                         row_text: &RowText,
                         paragraph: &Paragraph,
                         range: Range<usize>,
                         line_start: usize| {
            let (a, b) = (range.start.max(line_start + r.start), range.end.min(line_start + r.end));
            let through_newline = range.end > line_start + r.end && r.last;
            if a > b || (a == b && !through_newline) {
                return None;
            }
            let x0 = x_of(row_text, paragraph, a - line_start);
            let mut x1 = x_of(row_text, paragraph, b - line_start);
            if through_newline {
                x1 += m.char * 0.6;
            }
            Some(Rectangle::new(
                Point::new(origin_x + x0, row_y(row)),
                Size::new((x1 - x0).max(1.0), m.line),
            ))
        };

        let selections = self.editor.selections();
        let primary = selections.primary();
        let focused = state.focus.is_some();

        renderer.with_layer(bounds, |renderer| {
            fill(renderer, bounds, c.background);
        });

        renderer.with_layer(text_area, |renderer| {
            // The current line, when nothing is selected.
            if primary.is_empty() {
                let line = self.editor.line_of(primary.head);
                for (row, r, ..) in &rows {
                    if r.line == line {
                        let rect = Rectangle::new(
                            Point::new(text_area.x, row_y(*row)),
                            Size::new(text_area.width, m.line),
                        );
                        fill(renderer, rect, c.hover);
                    }
                }
            }
            for (row, r, row_text, paragraph) in &rows {
                let line_start = self.editor.line_start(r.line);
                let (row_start, row_end) = (line_start + r.start, line_start + r.end);
                // Search matches on this row.
                let from = self.matches.partition_point(|m| m.end < row_start);
                for (i, range) in self.matches[from..].iter().enumerate() {
                    if range.start > row_end {
                        break;
                    }
                    let current = self.current_match == Some(from + i);
                    let color = with_alpha(c.accent, if current { 0.45 } else { 0.2 });
                    if let Some(rect) =
                        span_rect(*row, r, row_text, paragraph, range.clone(), line_start)
                    {
                        fill(renderer, rect, color);
                    }
                }
                // The bracket pair at the cursor.
                if primary.is_empty()
                    && let Some((open, close)) = self.editor.matching_bracket(primary.head)
                {
                    for at in [open, close] {
                        if let Some(rect) =
                            span_rect(*row, r, row_text, paragraph, at..at + 1, line_start)
                        {
                            fill(renderer, rect, with_alpha(c.muted, 0.3));
                        }
                    }
                }
                for s in selections.iter().filter(|s| !s.is_empty()) {
                    if s.end() < row_start || s.start() > row_end {
                        continue;
                    }
                    if let Some(rect) =
                        span_rect(*row, r, row_text, paragraph, s.range(), line_start)
                    {
                        fill(renderer, rect, c.selection);
                    }
                }
                renderer.fill_paragraph(
                    paragraph,
                    Point::new(origin_x, row_y(*row)),
                    c.text,
                    text_area,
                );
            }
        });

        // Cursors, over the text.
        let blink = state.focus.as_ref().is_some_and(Focus::cursor_visible);
        if blink {
            renderer.with_layer(text_area, |renderer| {
                for s in selections.iter() {
                    if let Some((row, x)) = caret_on_screen(s, &rows, self.editor, &x_of) {
                        let rect = Rectangle::new(
                            Point::new(origin_x + x - 1.0, row_y(row)),
                            Size::new(2.0, m.line),
                        );
                        fill(renderer, rect, c.accent);
                    }
                }
            });
        }

        // Line numbers.
        let gutter = Rectangle::new(bounds.position(), Size::new(m.gutter, bounds.height));
        let marks: Vec<Mark> =
            self.git.map(|g| g.borrow_mut().marks(self.editor).to_vec()).unwrap_or_default();
        renderer.with_layer(gutter, |renderer| {
            // Changes since the last commit: a bar beside the line numbers,
            // or a wedge where lines were deleted.
            for (row, r, ..) in &rows {
                for mark in &marks {
                    let (color, hit) = match *mark {
                        Mark::Added { start, end } => (c.added, (start..end).contains(&r.line)),
                        Mark::Modified { start, end } => {
                            (c.modified, (start..end).contains(&r.line))
                        }
                        Mark::Deleted { .. } => continue,
                    };
                    if hit {
                        let rect = Rectangle::new(
                            Point::new(bounds.x + 4.0, row_y(*row)),
                            Size::new(3.0, m.line),
                        );
                        fill(renderer, rect, color);
                    }
                }
            }
            for mark in &marks {
                if let Mark::Deleted { at } = *mark
                    && let Some((row, ..)) =
                        rows.iter().find(|(_, r, ..)| r.line == at && r.start == 0)
                {
                    let rect = Rectangle::new(
                        Point::new(bounds.x + 2.0, row_y(*row) - 2.0),
                        Size::new(8.0, 4.0),
                    );
                    fill(renderer, rect, c.danger);
                }
            }
            for (row, line, paragraph) in &numbers {
                let color = if *line == current_line && focused { c.text } else { c.muted };
                let x = bounds.x + m.gutter - GUTTER_RIGHT - paragraph.min_width();
                renderer.fill_paragraph(paragraph, Point::new(x, row_y(*row)), color, gutter);
            }
        });

        // Scrollbars, when there's somewhere to scroll.
        renderer.with_layer(bounds, |renderer| {
            for bar in self.bars(&m, max, offset).into_iter().flatten() {
                let (_, thumb) = bar;
                let thumb = Rectangle::new(
                    Point::new(bounds.x + thumb.x, bounds.y + thumb.y),
                    thumb.size(),
                );
                renderer.fill_quad(
                    Quad {
                        bounds: thumb,
                        border: Border { radius: 3.0.into(), ..Border::default() },
                        shadow: Shadow::default(),
                        snap: true,
                    },
                    Background::Color(with_alpha(c.muted, 0.35)),
                );
            }
        });
    }
}

/// The row and x of a selection's cursor, if it's on one of `rows`.
fn caret_on_screen(
    s: &Selection,
    rows: &[(usize, Row, RowText, Paragraph)],
    editor: &Editor,
    x_of: &impl Fn(&RowText, &Paragraph, usize) -> f32,
) -> Option<(usize, f32)> {
    let (line, col) = editor.line_col(s.head);
    rows.iter().find_map(|(row, r, row_text, paragraph)| {
        let here = r.line == line && col >= r.start && (col < r.end || (col == r.end && r.last));
        here.then(|| (*row, x_of(row_text, paragraph, col)))
    })
}

fn with_alpha(color: Color, alpha: f32) -> Color {
    Color { a: color.a * alpha, ..color }
}

impl<Message> CodeView<'_, Message> {
    /// The paragraph for a row, from the cache when the same text in the
    /// same colors was drawn before.
    fn cached_paragraph(
        &self,
        cache: &mut Cache,
        row_text: &RowText,
        r: &Row,
        style: &LineStyle,
    ) -> Paragraph {
        // Styles cut to this row, in `row_text.text` offsets.
        let mut runs: Vec<(Range<usize>, syntax::Style)> = Vec::new();
        for (range, s) in style.iter() {
            let (a, b) = (range.start.max(r.start), range.end.min(r.end));
            if a < b {
                runs.push((row_text.to_text(a)..row_text.to_text(b), *s));
            }
        }
        let mut hasher = std::hash::DefaultHasher::new();
        (&row_text.text, &runs, self.font, self.size.to_bits(), self.line_height.to_bits())
            .hash(&mut hasher);
        let key = hasher.finish();
        if let Some(p) = cache.paragraphs.get(&key) {
            return p.clone();
        }
        let text = &row_text.text;
        let mut spans: Vec<Span<'_, (), Font>> = Vec::new();
        let mut at = 0;
        for (range, s) in &runs {
            if range.start > at {
                spans.push(Span::new(&text[at..range.start]));
            }
            let mut font = self.font;
            if s.bold {
                font.weight = font::Weight::Bold;
            }
            if s.italic {
                font.style = font::Style::Italic;
            }
            let mut span = Span::new(&text[range.clone()]).font(font);
            if let Some(color) = s.color() {
                span = span.color(color);
            }
            spans.push(span);
            at = range.end;
        }
        if at < text.len() || spans.is_empty() {
            spans.push(Span::new(&text[at..]));
        }
        let paragraph = self.paragraph(text, &spans);
        cache.paragraphs.insert(key, paragraph.clone());
        paragraph
    }

    /// A line number, cached like the rows.
    fn number(&self, cache: &mut Cache, n: usize) -> Paragraph {
        let mut hasher = std::hash::DefaultHasher::new();
        ("gutter", n, self.font, self.size.to_bits(), self.line_height.to_bits()).hash(&mut hasher);
        let key = hasher.finish();
        if let Some(p) = cache.paragraphs.get(&key) {
            return p.clone();
        }
        let paragraph = self.paragraph(&n.to_string(), &[]);
        cache.paragraphs.insert(key, paragraph.clone());
        paragraph
    }

    fn drag_bar(
        &self,
        state: &mut State,
        m: &Metrics,
        max: Vector,
        point: Point,
        shell: &mut Shell<'_, Message>,
    ) {
        let [vertical, horizontal] = self.bars(m, max, state.offset);
        match state.bar {
            Some(Bar::Vertical { grab }) => {
                if let Some((track, thumb)) = vertical {
                    let room = (track.height - thumb.height).max(1.0);
                    state.offset.y = ((point.y - grab) / room * max.y).clamp(0.0, max.y);
                }
            }
            Some(Bar::Horizontal { grab }) => {
                if let Some((track, thumb)) = horizontal {
                    let room = (track.width - thumb.width).max(1.0);
                    state.offset.x = ((point.x - track.x - grab) / room * max.x).clamp(0.0, max.x);
                }
            }
            None => return,
        }
        shell.publish((self.on_scroll)(state.offset));
        shell.request_redraw();
    }

    fn input_method<'b>(
        &self,
        state: &'b State,
        m: &Metrics,
        bounds: Rectangle,
    ) -> InputMethod<&'b str> {
        let primary = self.editor.selections().primary();
        let (row, x) = self.caret(primary.head, m);
        let position = Point::new(
            bounds.x + m.text_x + x - state.offset.x,
            bounds.y + PAD_TOP + row as f32 * m.line - state.offset.y,
        );
        InputMethod::Enabled {
            cursor: Rectangle::new(position, Size::new(1.0, m.line)),
            purpose: input_method::Purpose::Normal,
            preedit: state.preedit.as_ref().map(input_method::Preedit::as_ref),
        }
    }
}

impl<'a, Message: 'a> From<CodeView<'a, Message>> for Element<'a, Message, Theme, Renderer> {
    fn from(view: CodeView<'a, Message>) -> Self {
        Element::new(view)
    }
}
