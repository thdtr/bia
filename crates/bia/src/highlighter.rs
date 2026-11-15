//! Bridges syntect (grammar + BIA theme) to iced's incremental highlighter.
//!
//! iced re-highlights from the first edited line onward; we keep a parser
//! snapshot every [`LINES_PER_SNAPSHOT`] lines so an edit near the end of a
//! long file does not re-parse it from the top.

use bia_core::language::syntect::highlighting::{FontStyle, Highlighter as Styler, Theme};
use bia_core::language::syntect::parsing::{ParseState, ScopeStack, ScopeStackOp, SyntaxSet};
use iced::advanced::text::highlighter::{self, Format};
use iced::{Color, Font, font};
use std::ops::Range;
use std::sync::Arc;

const LINES_PER_SNAPSHOT: usize = 64;

#[derive(Clone)]
pub struct Settings {
    pub syntax: String,
    pub syntaxes: Arc<SyntaxSet>,
    pub theme: Arc<Theme>,
    pub font: Font,
    /// Bumped whenever the theme or the grammar set is reloaded.
    pub generation: u64,
}

impl PartialEq for Settings {
    fn eq(&self, other: &Self) -> bool {
        self.syntax == other.syntax
            && self.generation == other.generation
            && self.font == other.font
    }
}

pub struct Highlighter {
    settings: Settings,
    /// `snapshots[k]` is the parser state at the start of line `k * N`.
    snapshots: Vec<(ParseState, ScopeStack)>,
    /// The state at the start of `current_line`.
    working: (ParseState, ScopeStack),
    current_line: usize,
}

fn initial_state(settings: &Settings) -> (ParseState, ScopeStack) {
    let set = &settings.syntaxes;
    let syntax =
        set.find_syntax_by_name(&settings.syntax).unwrap_or_else(|| set.find_syntax_plain_text());
    (ParseState::new(syntax), ScopeStack::new())
}

impl highlighter::Highlighter for Highlighter {
    type Settings = Settings;
    type Highlight = Format<Font>;
    type Iterator<'a> = std::vec::IntoIter<(Range<usize>, Format<Font>)>;

    fn new(settings: &Settings) -> Self {
        let initial = initial_state(settings);
        Self {
            snapshots: vec![initial.clone()],
            working: initial,
            settings: settings.clone(),
            current_line: 0,
        }
    }

    fn update(&mut self, settings: &Settings) {
        *self = Self::new(settings);
    }

    fn change_line(&mut self, line: usize) {
        let k = (line / LINES_PER_SNAPSHOT).min(self.snapshots.len() - 1);
        self.snapshots.truncate(k + 1);
        self.working = self.snapshots[k].clone();
        self.current_line = k * LINES_PER_SNAPSHOT;
    }

    fn highlight_line(&mut self, line: &str) -> Self::Iterator<'_> {
        if self.current_line.is_multiple_of(LINES_PER_SNAPSHOT)
            && self.current_line / LINES_PER_SNAPSHOT == self.snapshots.len()
        {
            self.snapshots.push(self.working.clone());
        }
        self.current_line += 1;

        let (parser, stack) = &mut self.working;
        let ops = parser.parse_line(line, &self.settings.syntaxes).unwrap_or_default();
        let styler = Styler::new(&self.settings.theme);
        spans(&ops, line, stack, &styler, self.settings.font).into_iter()
    }

    fn current_line(&self) -> usize {
        self.current_line
    }
}

fn spans(
    ops: &[(usize, ScopeStackOp)],
    line: &str,
    stack: &mut ScopeStack,
    styler: &Styler<'_>,
    font: Font,
) -> Vec<(Range<usize>, Format<Font>)> {
    let mut out = Vec::with_capacity(ops.len() + 1);
    let mut start = 0;
    let mut emit = |stack: &ScopeStack, end: usize, out: &mut Vec<_>| {
        if end > start {
            out.push((start..end, format(styler, stack, font)));
        }
        start = end;
    };
    for (index, op) in ops {
        let index = (*index).min(line.len());
        emit(stack, index, &mut out);
        let _ = stack.apply(op);
    }
    emit(stack, line.len(), &mut out);
    out
}

fn format(styler: &Styler<'_>, stack: &ScopeStack, font: Font) -> Format<Font> {
    let style = styler.style_mod_for_stack(stack.as_slice());
    let color = style.foreground.map(|c| Color::from_rgba8(c.r, c.g, c.b, f32::from(c.a) / 255.0));
    let font = style.font_style.and_then(|fs| {
        let bold = fs.contains(FontStyle::BOLD);
        let italic = fs.contains(FontStyle::ITALIC);
        (bold || italic).then_some(Font {
            weight: if bold { font::Weight::Bold } else { font.weight },
            style: if italic { font::Style::Italic } else { font.style },
            ..font
        })
    });
    Format { color, font }
}
