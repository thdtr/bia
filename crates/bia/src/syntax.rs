//! Syntax highlighting for the editor: syntect colors, line by line.
//!
//! syntect parses lines in order, so the parser state is saved every
//! [`SNAPSHOT_EVERY`] lines. Drawing asks only for the lines on screen; an
//! edit drops what comes after the first changed line and the next draw
//! re-parses from the snapshot before it.
//!
//! Reaching line 300,000 of a file would mean parsing all the lines before
//! it, which takes most of a minute. So each draw parses for [`BUDGET`] at
//! most and carries on in the next frame, and the lines on screen get a
//! guess meanwhile: parsed from their own first line. More than [`FAR`]
//! lines past what was parsed, BIA doesn't go back at all: it starts
//! parsing a little above the lines on screen. Both are right except inside
//! a comment or string longer than that.

use bia_core::editor::Editor;
use bia_core::language::syntect::highlighting::{FontStyle, Highlighter, Theme};
use bia_core::language::syntect::parsing::{ParseState, ScopeStack, ScopeStackOp, SyntaxSet};
use iced::Color;
use std::collections::{BTreeMap, HashMap};
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

#[derive(Default)]
pub struct Highlights {
    key: Option<(String, u64)>,
    revision: u64,
    /// `snapshots[k]`: parser state at the start of line `k * SNAPSHOT_EVERY`.
    /// Line 0 always has one.
    snapshots: BTreeMap<usize, (ParseState, ScopeStack)>,
    /// The grammar's state at the start of a file.
    initial: Option<ParseState>,
    lines: HashMap<usize, LineStyle>,
    /// Lines styled from a guessed start, until real parsing reaches them.
    guesses: HashMap<usize, LineStyle>,
    /// The last request couldn't be answered fully in time.
    pending: bool,
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
        let missing = (lines.start..end).find(|l| !self.lines.contains_key(l));
        self.pending = false;
        if let Some(first) = missing {
            let done = self.parse(editor, cx, first, end, Instant::now() + BUDGET);
            if !done {
                self.pending = true;
                self.guess(editor, cx, lines.start..end);
            }
        }
        (lines.start..end)
            .map(|l| {
                self.lines.get(&l).or_else(|| self.guesses.get(&l)).cloned().unwrap_or_default()
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

    /// Drops what the edits since last time made stale.
    fn sync(&mut self, editor: &Editor, cx: Context<'_>) {
        let key = (editor.syntax.clone(), cx.generation);
        if self.key.as_ref() != Some(&key) {
            let syntax = cx
                .syntaxes
                .find_syntax_by_name(&key.0)
                .unwrap_or_else(|| cx.syntaxes.find_syntax_plain_text());
            let initial = ParseState::new(syntax);
            self.snapshots = BTreeMap::from([(0, (initial.clone(), ScopeStack::new()))]);
            self.initial = Some(initial);
            self.lines.clear();
            self.guesses.clear();
            self.key = Some(key);
            self.revision = editor.revision();
            return;
        }
        if let Some(line) = editor.first_changed_line(self.revision) {
            self.snapshots.split_off(&(line / SNAPSHOT_EVERY + 1));
            self.lines.retain(|&l, _| l < line);
            self.guesses.clear();
        }
        self.revision = editor.revision();
        if self.lines.len() > MAX_LINES {
            self.lines.clear();
        }
        if self.guesses.len() > MAX_LINES {
            self.guesses.clear();
        }
    }

    /// Parses from the snapshot at or before `from` up to `to`, keeping the
    /// styles of lines from `from` on. Stops at `deadline`, returning false;
    /// the snapshots it saved let the next call carry on.
    fn parse(
        &mut self,
        editor: &Editor,
        cx: Context<'_>,
        from: usize,
        to: usize,
        deadline: Instant,
    ) -> bool {
        let want = from / SNAPSHOT_EVERY;
        let Some((&nearest, _)) = self.snapshots.range(..=want).next_back() else { return true };
        let k = if (want - nearest) * SNAPSHOT_EVERY > FAR
            && let Some(initial) = &self.initial
        {
            let k = from.saturating_sub(RESTART_ABOVE) / SNAPSHOT_EVERY;
            self.snapshots.insert(k, (initial.clone(), ScopeStack::new()));
            k
        } else {
            nearest
        };
        let (mut parser, mut stack) = self.snapshots[&k].clone();
        let highlighter = Highlighter::new(cx.theme);
        for line in k * SNAPSHOT_EVERY..to {
            if line % SNAPSHOT_EVERY == 0 {
                if line > k * SNAPSHOT_EVERY {
                    self.snapshots.insert(line / SNAPSHOT_EVERY, (parser.clone(), stack.clone()));
                }
                if Instant::now() > deadline {
                    return false;
                }
            }
            // The grammars are the "no newlines" build: lines go in without
            // their `\n`, which also parses faster.
            let text = editor.line(line);
            let ops = parser.parse_line(&text, cx.syntaxes).unwrap_or_default();
            let styled = styles(&ops, text.len(), &mut stack, &highlighter);
            if line >= from {
                self.lines.insert(line, Rc::new(styled));
                self.guesses.remove(&line);
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

    #[test]
    fn edits_reparse_from_the_changed_line() {
        let (syntaxes, _) = language::load_syntaxes(std::path::Path::new("/nonexistent"));
        let theme =
            language::syntax_theme(&bia_core::Theme::load("noite", std::path::Path::new("/x")).0);
        let cx = Context { syntaxes: &syntaxes, theme: &theme, generation: 0 };
        let mut ed = Editor::new("/* a\nb\nc */\n", Indent::spaces(4));
        ed.syntax = "Rust".into();
        let mut h = Highlights::default();
        let before = h.lines(&ed, cx, 0..3);
        assert_eq!(before.len(), 3);
        // Inside the comment, line 1 is one color.
        assert_eq!(before[1].len(), 1);

        // Closing the comment on line 0 changes how line 1 looks.
        ed.select(4..4);
        ed.insert(" */");
        let after = h.lines(&ed, cx, 1..2);
        assert_ne!(after[0], before[1]);
    }

    #[test]
    fn far_lines_start_parsing_near_themselves() {
        let (syntaxes, _) = language::load_syntaxes(std::path::Path::new("/nonexistent"));
        let theme =
            language::syntax_theme(&bia_core::Theme::load("noite", std::path::Path::new("/x")).0);
        let cx = Context { syntaxes: &syntaxes, theme: &theme, generation: 0 };
        let text = "let x = 1;\n".repeat(50_000);
        let mut ed = Editor::new(&text, Indent::spaces(4));
        ed.syntax = "Rust".into();
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
        let skipped = 1..(40_000 / SNAPSHOT_EVERY);
        assert_eq!(h.snapshots.range(skipped).count(), 0);
        assert_eq!(far[0], h.lines(&ed, cx, 0..1)[0]);
    }
}
