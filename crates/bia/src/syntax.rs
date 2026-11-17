//! Syntax highlighting for the editor: syntect colors, line by line.
//!
//! syntect parses lines in order, from a state that depends on every line
//! before. Each highlighted line keeps the state it started from, and the
//! state is also saved every [`SNAPSHOT_EVERY`] lines, so parsing can start
//! close to any line. Drawing asks only for the lines on screen.
//!
//! An edit doesn't throw away what comes after it: those lines are set
//! aside ([`Stale`]), moved to where they are now. Parsing goes on from the
//! edited lines, and as soon as the parser's state at the start of a line
//! is the same as before, everything set aside from there on holds again.
//! Typing inside a line costs a line or two of parsing; opening a comment
//! reparses what it covers.
//!
//! Reaching line 300,000 of a file would mean parsing all the lines before
//! it, which takes most of a minute. So each draw parses for [`BUDGET`] at
//! most and carries on in the next frame, and the lines on screen get a
//! guess meanwhile: parsed from their own first line. More than [`FAR`]
//! lines past what was parsed, BIA doesn't go back at all: it starts
//! parsing a little above the lines on screen. Both are right except inside
//! a comment or string longer than that.

use bia_core::editor::{Changed, Editor};
use bia_core::language::syntect::highlighting::{FontStyle, Highlighter, Theme};
use bia_core::language::syntect::parsing::{ParseState, ScopeStack, ScopeStackOp, SyntaxSet};
use iced::Color;
use std::collections::{BTreeMap, HashMap};
use std::mem;
use std::ops::Range;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// Parsing time per frame.
const BUDGET: Duration = Duration::from_millis(12);

/// Lines past the last parsed one beyond which parsing starts over near
/// the lines asked for, this many lines above them.
const FAR: usize = 5_000;
const RESTART_ABOVE: usize = 200;

const SNAPSHOT_EVERY: usize = 16;

/// Highlighted lines kept before the cache starts over.
const MAX_LINES: usize = 4_000;

#[derive(Debug, Clone, Copy, PartialEq, Hash)]
pub struct Style {
    pub color: Option<[u8; 4]>,
    pub bold: bool,
    pub italic: bool,
}

impl Style {
    pub fn color(&self) -> Option<Color> {
        self.color.map(|[r, g, b, a]| Color::from_rgba8(r, g, b, f32::from(a) / 255.0))
    }
}

/// Byte ranges of one line and how each looks.
pub type LineStyle = Rc<Vec<(Range<usize>, Style)>>;

/// What the highlighting depends on besides the text.
#[derive(Clone, Copy)]
pub struct Context<'a> {
    pub syntaxes: &'a SyntaxSet,
    pub theme: &'a Theme,
    /// Bumped when themes or grammars are reloaded.
    pub generation: u64,
}

/// The parser's state at the start of a line.
type State = (ParseState, ScopeStack);

/// A highlighted line and the state it was parsed from.
#[derive(Clone)]
struct Line {
    style: LineStyle,
    start: Rc<State>,
}

/// What was known past the last edit, moved to the lines it's on now. It
/// holds again from the first line where parsing reaches the same state.
#[derive(Default)]
struct Stale {
    lines: HashMap<usize, Line>,
    snapshots: BTreeMap<usize, State>,
}

impl Stale {
    fn starts_with(&self, line: usize, state: &State) -> bool {
        let old = self.lines.get(&line).map(|l| &*l.start).or_else(|| self.snapshots.get(&line));
        old == Some(state)
    }
}

#[derive(Default)]
pub struct Highlights {
    key: Option<(String, u64)>,
    revision: u64,
    /// Parser state at the start of some lines. Line 0 always has one.
    snapshots: BTreeMap<usize, State>,
    /// The grammar's state at the start of a file.
    initial: Option<ParseState>,
    lines: HashMap<usize, Line>,
    stale: Stale,
    /// Lines styled from a guessed start, until real parsing reaches them.
    guesses: HashMap<usize, LineStyle>,
    /// The last request couldn't be answered fully in time.
    pending: bool,
    /// Lines parsed so far, for tests.
    #[cfg(test)]
    parsed: usize,
}

impl Highlights {
    /// Styles for `lines` of `editor`, in order.
    pub fn lines(
        &mut self,
        editor: &Editor,
        cx: Context<'_>,
        lines: Range<usize>,
    ) -> Vec<LineStyle> {
        self.sync(editor, cx);
        let end = lines.end.min(editor.line_count());
        let deadline = Instant::now() + BUDGET;
        self.pending = false;
        // Twice at most: the first pass may stop early, where the lines set
        // aside by an edit turn out to hold again.
        for _ in 0..2 {
            let Some(first) = (lines.start..end).find(|l| !self.lines.contains_key(l)) else {
                break;
            };
            if !self.parse(editor, cx, first, end, deadline) {
                self.pending = true;
                self.guess(editor, cx, lines.start..end);
                break;
            }
        }
        (lines.start..end)
            .map(|l| {
                self.lines
                    .get(&l)
                    .map(|line| &line.style)
                    .or_else(|| self.guesses.get(&l))
                    .cloned()
                    .unwrap_or_default()
            })
            .collect()
    }

    /// Whether more parsing is needed to style what was last asked for.
    pub fn pending(&self) -> bool {
        self.pending
    }

    /// Styles `lines` parsing from the first of them, as if a file started
    /// there.
    fn guess(&mut self, editor: &Editor, cx: Context<'_>, lines: Range<usize>) {
        if lines.clone().all(|l| self.guesses.contains_key(&l) || self.lines.contains_key(&l)) {
            return;
        }
        let Some(parser) = &self.initial else { return };
        let (mut parser, mut stack) = (parser.clone(), ScopeStack::new());
        let highlighter = Highlighter::new(cx.theme);
        for line in lines {
            let text = editor.line(line);
            let ops = parser.parse_line(&text, cx.syntaxes).unwrap_or_default();
            let styled = styles(&ops, text.len(), &mut stack, &highlighter);
            self.guesses.insert(line, Rc::new(styled));
        }
    }

    /// Catches up with the edits made since last time.
    fn sync(&mut self, editor: &Editor, cx: Context<'_>) {
        let key = (editor.syntax.clone(), cx.generation);
        if self.key.as_ref() != Some(&key) {
            let syntax = cx
                .syntaxes
                .find_syntax_by_name(&key.0)
                .unwrap_or_else(|| cx.syntaxes.find_syntax_plain_text());
            self.initial = Some(ParseState::new(syntax));
            self.key = Some(key);
            self.forget();
        } else {
            match editor.changed_lines(self.revision) {
                Changed::Nothing => {}
                Changed::Everything => self.forget(),
                Changed::Lines { first, old_end, new_end } => {
                    self.edited(first, old_end, new_end);
                }
            }
        }
        self.revision = editor.revision();
        if self.lines.len() > MAX_LINES {
            self.lines.clear();
        }
        if self.stale.lines.len() > MAX_LINES {
            self.stale = Stale::default();
        }
        if self.guesses.len() > MAX_LINES {
            self.guesses.clear();
        }
    }

    /// Starts over from the top of the file.
    fn forget(&mut self) {
        let initial = self.initial.clone().map(|p| (p, ScopeStack::new()));
        self.snapshots = initial.into_iter().map(|s| (0, s)).collect();
        self.lines.clear();
        self.stale = Stale::default();
        self.guesses.clear();
    }

    /// Lines `first..old_end` became `first..new_end`. What came after them
    /// is set aside, moved by as many lines as were added or removed.
    fn edited(&mut self, first: usize, old_end: usize, new_end: usize) {
        let moved = |l: usize| (l + new_end).saturating_sub(old_end);
        // What an earlier edit set aside no longer holds past this one.
        self.stale.lines.retain(|&l, _| l < first);
        self.stale.snapshots.retain(|&l, _| l < first);
        for (l, line) in mem::take(&mut self.lines) {
            if l < first {
                self.lines.insert(l, line);
            } else if l >= old_end {
                self.stale.lines.insert(moved(l), line);
            }
        }
        // The state at the start of `first` only depends on the lines above.
        for (l, state) in self.snapshots.split_off(&(first + 1)) {
            if l >= old_end {
                self.stale.snapshots.insert(moved(l), state);
            }
        }
        self.guesses.clear();
    }

    /// Takes back what was set aside, from `line` on: parsing reached it
    /// in the same state as before the edit.
    fn take_back(&mut self, line: usize) {
        let stale = mem::take(&mut self.stale);
        self.lines.extend(stale.lines.into_iter().filter(|&(l, _)| l >= line));
        self.snapshots.extend(stale.snapshots.into_iter().filter(|&(l, _)| l >= line));
    }

    /// Parses from the closest known state at or before `from` up to `to`.
    /// Returns false if it stopped at `deadline`: what it saved lets the
    /// next call carry on. Returns true early where the lines set aside by
    /// an edit hold again.
    fn parse(
        &mut self,
        editor: &Editor,
        cx: Context<'_>,
        from: usize,
        to: usize,
        deadline: Instant,
    ) -> bool {
        let Some(&nearest) = self.snapshots.range(..=from).next_back().map(|(l, _)| l) else {
            return true;
        };
        let snapshot = if from - nearest > FAR
            && let Some(initial) = &self.initial
        {
            let restart = from.saturating_sub(RESTART_ABOVE);
            self.snapshots.insert(restart, (initial.clone(), ScopeStack::new()));
            restart
        } else {
            nearest
        };
        // A highlighted line between the snapshot and `from` is closer.
        let (begin, state) = (snapshot + 1..=from)
            .rev()
            .find_map(|l| self.lines.get(&l).map(|line| (l, (*line.start).clone())))
            .unwrap_or_else(|| (snapshot, self.snapshots[&snapshot].clone()));
        let (mut parser, mut stack) = state;
        let highlighter = Highlighter::new(cx.theme);
        for line in begin..to {
            let start = (parser, stack);
            if line > begin {
                if self.stale.starts_with(line, &start) {
                    self.take_back(line);
                    return true;
                }
                if line % SNAPSHOT_EVERY == 0 {
                    self.snapshots.insert(line, start.clone());
                }
                if Instant::now() > deadline {
                    return false;
                }
            }
            let start = Rc::new(start);
            (parser, stack) = (*start).clone();
            let text = editor.line(line);
            let ops = parser.parse_line(&text, cx.syntaxes).unwrap_or_default();
            let style = Rc::new(styles(&ops, text.len(), &mut stack, &highlighter));
            self.lines.insert(line, Line { style, start });
            self.guesses.remove(&line);
            #[cfg(test)]
            {
                self.parsed += 1;
            }
        }
        true
    }
}

fn styles(
    ops: &[(usize, ScopeStackOp)],
    len: usize,
    stack: &mut ScopeStack,
    highlighter: &Highlighter<'_>,
) -> Vec<(Range<usize>, Style)> {
    let mut out: Vec<(Range<usize>, Style)> = Vec::with_capacity(ops.len() + 1);
    let mut start = 0;
    let mut emit = |stack: &ScopeStack, end: usize, out: &mut Vec<(Range<usize>, Style)>| {
        let end = end.min(len);
        if end > start {
            let style = style(highlighter, stack);
            match out.last_mut() {
                Some((range, last)) if *last == style && range.end == start => range.end = end,
                _ => out.push((start..end, style)),
            }
            start = end;
        }
    };
    for (index, op) in ops {
        emit(stack, *index, &mut out);
        let _ = stack.apply(op);
    }
    emit(stack, len, &mut out);
    out
}

fn style(highlighter: &Highlighter<'_>, stack: &ScopeStack) -> Style {
    let s = highlighter.style_mod_for_stack(stack.as_slice());
    let font = s.font_style.unwrap_or_else(FontStyle::empty);
    Style {
        color: s.foreground.map(|c| [c.r, c.g, c.b, c.a]),
        bold: font.contains(FontStyle::BOLD),
        italic: font.contains(FontStyle::ITALIC),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bia_core::indent::Indent;
    use bia_core::language;

    fn with_context(test: impl FnOnce(Context<'_>)) {
        let (syntaxes, _) = language::load_syntaxes(std::path::Path::new("/nonexistent"));
        let theme =
            language::syntax_theme(&bia_core::Theme::load("noite", std::path::Path::new("/x")).0);
        test(Context { syntaxes: &syntaxes, theme: &theme, generation: 0 });
    }

    fn rust(text: &str) -> Editor {
        let mut ed = Editor::new(text, Indent::spaces(4));
        ed.syntax = "Rust".into();
        ed
    }

    /// Styles of `lines`, asking again until parsing is done, as drawing
    /// does frame after frame.
    fn settled(
        h: &mut Highlights,
        ed: &Editor,
        cx: Context<'_>,
        lines: Range<usize>,
    ) -> Vec<LineStyle> {
        loop {
            let styles = h.lines(ed, cx, lines.clone());
            if !h.pending() {
                return styles;
            }
        }
    }

    /// Styles of `lines`, from a cache that never saw an edit.
    fn fresh(ed: &Editor, cx: Context<'_>, lines: Range<usize>) -> Vec<LineStyle> {
        settled(&mut Highlights::default(), ed, cx, lines)
    }

    #[test]
    fn edits_reparse_from_the_changed_line() {
        with_context(|cx| {
            let mut ed = rust("/* a\nb\nc */\n");
            let mut h = Highlights::default();
            let before = settled(&mut h, &ed, cx, 0..3);
            assert_eq!(before.len(), 3);
            // Inside the comment, line 1 is one color.
            assert_eq!(before[1].len(), 1);

            // Closing the comment on line 0 changes how line 1 looks.
            ed.select(4..4);
            ed.insert(" */");
            let after = settled(&mut h, &ed, cx, 1..2);
            assert_ne!(after[0], before[1]);
            assert_eq!(settled(&mut h, &ed, cx, 0..4), fresh(&ed, cx, 0..4));
        });
    }

    #[test]
    fn typing_in_a_line_parses_little() {
        with_context(|cx| {
            let text = "fn f() {\n".to_owned() + &"    let x = \"a\"; // b\n".repeat(2_000) + "}\n";
            let mut ed = rust(&text);
            let mut h = Highlights::default();
            settled(&mut h, &ed, cx, 0..60);
            let parsed = h.parsed;
            // Type in line 30.
            let at = ed.line_start(30) + 8;
            ed.select(at..at);
            ed.insert("y");
            assert_eq!(settled(&mut h, &ed, cx, 0..60), fresh(&ed, cx, 0..60));
            assert!(h.parsed - parsed <= 3, "parsed {} lines", h.parsed - parsed);
            // Adding a line moves what's below without parsing it again.
            let parsed = h.parsed;
            let at = ed.line_end(10);
            ed.select(at..at);
            ed.insert("\n    let z = 2;");
            assert_eq!(settled(&mut h, &ed, cx, 0..61), fresh(&ed, cx, 0..61));
            assert!(h.parsed - parsed <= 4, "parsed {} lines", h.parsed - parsed);
        });
    }

    #[test]
    fn edits_that_change_what_follows_are_followed() {
        with_context(|cx| {
            let text = "let a = 1;\n".repeat(100);
            let mut ed = rust(&text);
            let mut h = Highlights::default();
            settled(&mut h, &ed, cx, 0..100);
            // Opening a comment changes every line after it.
            let at = ed.line_start(20);
            ed.select(at..at);
            ed.insert("/* ");
            assert_eq!(settled(&mut h, &ed, cx, 0..100), fresh(&ed, cx, 0..100));
            // Two edits before the next draw: a line removed far below, then
            // the comment closed.
            let at = ed.line_start(80);
            ed.select(at..ed.line_start(81));
            ed.insert("");
            ed.select(ed.line_end(40)..ed.line_end(40));
            ed.insert(" */");
            assert_eq!(settled(&mut h, &ed, cx, 0..99), fresh(&ed, cx, 0..99));
            // Undo all of it.
            while ed.undo() {}
            assert_eq!(settled(&mut h, &ed, cx, 0..100), fresh(&ed, cx, 0..100));
        });
    }

    #[test]
    fn far_lines_start_parsing_near_themselves() {
        with_context(|cx| {
            let ed = rust(&"let x = 1;\n".repeat(50_000));
            let mut h = Highlights::default();
            let mut far = h.lines(&ed, cx, 49_000..49_040);
            for _ in 0..100 {
                if !h.pending() {
                    break;
                }
                far = h.lines(&ed, cx, 49_000..49_040);
            }
            assert!(!h.pending());
            // Nothing between the top and the restart point was parsed.
            assert_eq!(h.snapshots.range(1..40_000).count(), 0);
            assert_eq!(far[0], h.lines(&ed, cx, 0..1)[0]);
        });
    }
}
