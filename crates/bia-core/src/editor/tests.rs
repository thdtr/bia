//! Text in these tests marks cursors with `|` and selections with `[` and
//! `]` (the head is at `]` unless written `]<`, which puts it at `[`).

use super::*;

fn editor(marked: &str) -> Editor {
    let mut text = String::new();
    let mut list = Vec::new();
    let mut open = None;
    let mut chars = marked.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '|' => list.push(Selection::cursor(text.len())),
            '[' => open = Some(text.len()),
            ']' => {
                let start = open.take().expect("] without [");
                if chars.peek() == Some(&'<') {
                    chars.next();
                    list.push(Selection::new(text.len(), start));
                } else {
                    list.push(Selection::new(start, text.len()));
                }
            }
            c => text.push(c),
        }
    }
    let mut ed = Editor::new(&text, Indent::spaces(4));
    ed.syntax = "Rust".into();
    if !list.is_empty() {
        ed.set_selections(Selections::new(list, 0));
    }
    ed
}

fn show(ed: &Editor) -> String {
    let text = ed.text();
    let mut marks: Vec<(usize, &str)> = Vec::new();
    for s in ed.selections().iter() {
        if s.is_empty() {
            marks.push((s.head, "|"));
        } else {
            marks.push((s.start(), "["));
            marks.push((s.end(), if s.head < s.anchor { "]<" } else { "]" }));
        }
    }
    marks.sort_by_key(|&(at, m)| (at, m == "["));
    let mut out = String::new();
    let mut from = 0;
    for (at, mark) in marks {
        out.push_str(&text[from..at]);
        out.push_str(mark);
        from = at;
    }
    out.push_str(&text[from..]);
    out
}

fn typed(ed: &mut Editor, text: &str) {
    for c in text.chars() {
        ed.type_char(c);
    }
}

#[test]
fn notation_roundtrips() {
    for s in ["a|b", "[ab]c|", "[ab]<", "|"] {
        assert_eq!(show(&editor(s)), s);
    }
}

#[test]
fn typing_at_every_cursor() {
    let mut ed = editor("a|b\nc|d\n|");
    typed(&mut ed, "xy");
    assert_eq!(show(&ed), "axy|b\ncxy|d\nxy|");
    assert!(ed.is_dirty());
    assert!(ed.undo());
    assert_eq!(show(&ed), "a|b\nc|d\n|");
    assert!(!ed.is_dirty());
    assert!(ed.redo());
    assert_eq!(ed.text(), "axyb\ncxyd\nxy");
}

#[test]
fn typing_groups_by_word() {
    let mut ed = editor("|");
    typed(&mut ed, "ab cd");
    ed.undo();
    assert_eq!(ed.text(), "ab");
    ed.undo();
    assert_eq!(ed.text(), "");
}

#[test]
fn pairs() {
    let mut ed = editor("|");
    typed(&mut ed, "f(\"x");
    assert_eq!(show(&ed), "f(\"x|\")");
    typed(&mut ed, "\")");
    assert_eq!(show(&ed), "f(\"x\")|");

    let mut ed = editor("[a] [b]");
    typed(&mut ed, "(");
    assert_eq!(show(&ed), "([a]) ([b])");

    let mut ed = editor("(|) {|}");
    ed.backspace();
    assert_eq!(show(&ed), "| |");

    let mut ed = editor("|x");
    typed(&mut ed, "(");
    assert_eq!(show(&ed), "(|x");
}

#[test]
fn enter_and_backspace_indentation() {
    let mut ed = editor("fn a() {|}\n    x|");
    ed.newline();
    assert_eq!(show(&ed), "fn a() {\n    |\n}\n    x\n    |");
    ed.backspace();
    assert_eq!(show(&ed), "fn a() {\n|\n}\n    x\n|");
}

#[test]
fn backspace_and_delete_whole_graphemes() {
    let mut ed = editor("a👍🏽|b");
    ed.backspace();
    assert_eq!(show(&ed), "a|b");
    let mut ed = editor("a|🇧🇷b");
    ed.delete();
    assert_eq!(show(&ed), "a|b");
    let mut ed = editor("foo bar|");
    ed.delete_word(false);
    assert_eq!(show(&ed), "foo |");
}

#[test]
fn motions() {
    let mut ed = editor("  foo|\n\tx");
    ed.move_cursors(Motion::LineStart, false);
    assert_eq!(show(&ed), "  |foo\n\tx");
    ed.move_cursors(Motion::LineStart, false);
    assert_eq!(show(&ed), "|  foo\n\tx");
    ed.move_cursors(Motion::LineEnd, true);
    assert_eq!(show(&ed), "[  foo]\n\tx");
    ed.move_cursors(Motion::Left, false);
    assert_eq!(show(&ed), "|  foo\n\tx");

    // Up and down keep the display column across a tab (4 wide here).
    let mut ed = editor("abcd|e\n\tx\nabcdef");
    ed.move_cursors(Motion::Down, false);
    assert_eq!(show(&ed), "abcde\n\t|x\nabcdef");
    ed.move_cursors(Motion::Down, false);
    assert_eq!(show(&ed), "abcde\n\tx\nabcd|ef");
    ed.move_cursors(Motion::Down, false);
    assert_eq!(show(&ed), "abcde\n\tx\nabcdef|");
}

#[test]
fn comments_per_block() {
    let mut ed = editor("|a\nb\n\n  c|\nd");
    assert!(ed.toggle_comment());
    assert_eq!(show(&ed), "|// a\nb\n\n  // c|\nd");
    ed.toggle_comment();
    assert_eq!(show(&ed), "|a\nb\n\n  c|\nd");
    let mut plain = editor("|x");
    plain.syntax = "Plain Text".into();
    assert!(!plain.toggle_comment());
}

#[test]
fn indent_cursors_and_lines() {
    let mut ed = editor("a|b\nc|d");
    ed.indent();
    assert_eq!(show(&ed), "a   |b\nc   |d");
    let mut ed = editor("[a\nb]\nc");
    ed.indent();
    assert_eq!(show(&ed), "[    a\n    b]\nc");
    ed.outdent();
    assert_eq!(show(&ed), "[a\nb]\nc");
}

#[test]
fn move_lines() {
    let mut ed = editor("a\nb|\nc\nd|");
    ed.move_lines(true);
    assert_eq!(show(&ed), "b|\na\nd|\nc");
    ed.move_lines(true);
    assert_eq!(show(&ed), "b|\na\nd|\nc");
    let mut ed = editor("[a\n]b\nc");
    ed.move_lines(false);
    assert_eq!(show(&ed), "b\n[a\n]c");
    ed.move_lines(false);
    assert_eq!(show(&ed), "b\nc\n[a]");
    ed.undo();
    ed.undo();
    assert_eq!(show(&ed), "[a\n]b\nc");
}

#[test]
fn duplicate_lines() {
    let mut ed = editor("a|\nb\nc|");
    ed.duplicate_lines(true);
    assert_eq!(show(&ed), "a\na|\nb\nc\nc|");
    let mut ed = editor("[a\n]b");
    ed.duplicate_lines(false);
    assert_eq!(show(&ed), "[a\n]a\nb");
}

#[test]
fn delete_lines() {
    let mut ed = editor("abc\nd|ef\nghi\nx|");
    ed.delete_lines();
    // Both cursors land on "ghi": one from below it, one from the end.
    assert_eq!(show(&ed), "abc\ng|hi");
    let mut ed = editor("|only");
    ed.delete_lines();
    assert_eq!(show(&ed), "|");
}

#[test]
fn select_lines_grows() {
    let mut ed = editor("a|\nb\nc");
    ed.select_lines();
    assert_eq!(show(&ed), "[a\n]b\nc");
    ed.select_lines();
    assert_eq!(show(&ed), "[a\nb\n]c");
    ed.select_lines();
    assert_eq!(show(&ed), "[a\nb\nc]");
}

#[test]
fn next_occurrence() {
    let mut ed = editor("foo fo|o food foo");
    ed.select_next_occurrence();
    assert_eq!(show(&ed), "foo [foo] food foo");
    ed.select_next_occurrence();
    assert_eq!(show(&ed), "foo [foo] food [foo]");
    ed.select_next_occurrence();
    assert_eq!(show(&ed), "[foo] [foo] food [foo]");
    typed(&mut ed, "x");
    assert_eq!(show(&ed), "x| x| food x|");

    let mut ed = editor("[ab] ab xab");
    ed.select_all_occurrences();
    assert_eq!(show(&ed), "[ab] [ab] x[ab]");
    assert!(ed.collapse());
    assert_eq!(ed.selections().len(), 1);
}

#[test]
fn copy_paste() {
    // Nothing selected: whole lines, pasted above the cursor's line.
    let mut ed = editor("a|b\ncd");
    let copied = ed.copy();
    assert_eq!(copied, "ab\n");
    ed.move_cursors(Motion::Down, false);
    ed.paste(&copied);
    assert_eq!(show(&ed), "ab\nab\nc|d");

    // As many lines as cursors: one each.
    let mut ed = editor("|\n|");
    ed.paste("x\ny");
    assert_eq!(show(&ed), "x|\ny|");

    let mut ed = editor("[a]b\n[c]d");
    assert_eq!(ed.cut(), "a\nc");
    assert_eq!(show(&ed), "|b\n|d");
}

#[test]
fn add_cursor_vertically() {
    let mut ed = editor("abc\nab|c\na");
    ed.add_cursor_vertically(true);
    ed.add_cursor_vertically(false);
    assert_eq!(show(&ed), "ab|c\nab|c\na|");
    ed.add_cursor_vertically(true);
    assert_eq!(ed.selections().len(), 3);
}

#[test]
fn saved_state_survives_undo_and_redo() {
    let mut ed = editor("|");
    typed(&mut ed, "a");
    ed.seal();
    ed.mark_saved();
    typed(&mut ed, "b");
    assert!(ed.is_dirty());
    ed.undo();
    assert!(!ed.is_dirty());
    ed.undo();
    assert!(ed.is_dirty());
    ed.redo();
    assert!(!ed.is_dirty());
}

struct Rng(u64);

impl Rng {
    fn below(&mut self, n: usize) -> usize {
        self.0 =
            self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) as usize % n
    }
}

/// Random commands with several cursors. Selections stay valid, and undoing
/// everything gets back the original; redoing, the result.
#[test]
fn random_commands() {
    let original = "fn main() {\n    olá(\"日本\");\n\tlet x = 1;\n}\n\n👍🏽 fim";
    let motions = [
        Motion::Left,
        Motion::Right,
        Motion::WordLeft,
        Motion::WordRight,
        Motion::Up,
        Motion::Down,
        Motion::LineStart,
        Motion::LineEnd,
        Motion::DocumentStart,
        Motion::DocumentEnd,
        Motion::PageDown(3),
    ];
    let chars = ['a', ' ', 'é', '日', '(', ')', '"', '\'', '[', '\t', '\n'];
    for seed in 0..300 {
        let mut rng = Rng(seed * 7919 + 1);
        let mut ed = Editor::new(original, Indent::spaces(4));
        ed.syntax = "Rust".into();
        let mut map = DisplayMap::for_editor(&ed, Some(12));
        let mut log: Vec<String> = Vec::new();
        for _ in 0..80 {
            let n = rng.below(100);
            let step = match n {
                0..=24 => {
                    let c = chars[rng.below(chars.len())];
                    ed.type_char(c);
                    format!("type {c:?}")
                }
                25..=29 => {
                    ed.backspace();
                    "backspace".into()
                }
                30..=32 => {
                    ed.delete();
                    "delete".into()
                }
                33..=46 => {
                    let m = motions[rng.below(motions.len())];
                    let extend = rng.below(2) == 0;
                    ed.move_cursors(m, extend);
                    format!("move {m:?} {extend}")
                }
                47..=50 => {
                    ed.add_cursor_vertically(rng.below(2) == 0);
                    "add cursor".into()
                }
                51..=53 => {
                    ed.select_next_occurrence();
                    "ctrl+d".into()
                }
                54..=56 => {
                    ed.toggle_comment();
                    "comment".into()
                }
                57..=60 => {
                    ed.move_lines(rng.below(2) == 0);
                    "move lines".into()
                }
                61..=62 => {
                    ed.duplicate_lines(rng.below(2) == 0);
                    "duplicate".into()
                }
                63..=64 => {
                    ed.delete_lines();
                    "delete lines".into()
                }
                65..=66 => {
                    ed.select_lines();
                    "select lines".into()
                }
                67..=69 => {
                    if rng.below(2) == 0 {
                        ed.indent()
                    } else {
                        ed.outdent()
                    }
                    "indent".into()
                }
                70..=72 => {
                    let text = ed.copy();
                    ed.paste(&text);
                    "copy paste".into()
                }
                73..=74 => {
                    ed.cut();
                    "cut".into()
                }
                75..=77 => {
                    let at = rng.below(ed.len() + 1);
                    let at = ed.offset(ed.line_of(at), at - ed.line_start(ed.line_of(at)));
                    ed.toggle_cursor(at);
                    format!("alt+click {at}")
                }
                78..=79 => {
                    ed.collapse();
                    "escape".into()
                }
                80..=81 => {
                    ed.delete_word(rng.below(2) == 0);
                    "delete word".into()
                }
                82..=83 => {
                    ed.select_all_occurrences();
                    "select all occurrences".into()
                }
                84..=91 => {
                    ed.undo();
                    "undo".into()
                }
                _ => {
                    ed.redo();
                    "redo".into()
                }
            };
            log.push(step);
            check(&ed, seed, &log);
            // Updated edit by edit, the wrapped rows match a fresh count.
            map.sync(&ed, Some(12));
            assert_eq!(map, DisplayMap::for_editor(&ed, Some(12)), "seed {seed}: after {log:?}");
        }
        while ed.redo() {}
        let edited = ed.text();
        while ed.undo() {}
        assert_eq!(ed.text(), original, "seed {seed}: undo all after {log:?}");
        assert!(!ed.is_dirty(), "seed {seed}");
        while ed.redo() {}
        assert_eq!(ed.text(), edited, "seed {seed}: redo all after {log:?}");
    }
}

fn check(ed: &Editor, seed: u64, log: &[String]) {
    let text = ed.text();
    let list = ed.selections().as_slice();
    assert!(!list.is_empty());
    for s in list {
        for p in [s.anchor, s.head] {
            assert!(
                p <= text.len() && text.is_char_boundary(p),
                "seed {seed}: {p} in {text:?} after {log:?}"
            );
        }
    }
    for pair in list.windows(2) {
        assert!(
            pair[0].end() <= pair[1].start(),
            "seed {seed}: overlapping {list:?} after {log:?}"
        );
    }
    assert_eq!(ed.line_count(), text.split('\n').count());
}

#[test]
fn changed_lines_are_logged() {
    let mut ed = editor("a\nb\n|c");
    let start = ed.revision();
    assert_eq!(ed.first_changed_line(start), None);
    typed(&mut ed, "x");
    assert_eq!(ed.first_changed_line(start), Some(2));
    let mid = ed.revision();
    ed.move_cursors(Motion::DocumentStart, false);
    typed(&mut ed, "y");
    assert_eq!(ed.first_changed_line(mid), Some(0));
    let end = ed.revision();
    ed.undo();
    assert_eq!(ed.first_changed_line(end), Some(0));
}

#[test]
fn drag_by_word_and_line() {
    let mut ed = editor("foo bar baz\nqux");
    ed.drag(4..7, 9, Unit::Word);
    assert_eq!(show(&ed), "foo [bar baz]\nqux");
    ed.drag(4..7, 1, Unit::Word);
    assert_eq!(show(&ed), "[foo bar]< baz\nqux");
    ed.drag(0..0, 13, Unit::Line);
    assert_eq!(show(&ed), "[foo bar baz\nqux]");
}

#[test]
fn brackets_match() {
    // Not through `editor()`: its notation uses square brackets.
    let ed = Editor::new("f(a[1], {b})", Indent::spaces(4));
    assert_eq!(ed.matching_bracket(1), Some((1, 11)));
    assert_eq!(ed.matching_bracket(12), Some((1, 11)));
    assert_eq!(ed.matching_bracket(4), Some((3, 5)));
    assert_eq!(ed.matching_bracket(0), None);
}

#[test]
fn up_and_down_by_rows() {
    // The first line wraps into "aaaa bbbb " and "cccc dddd".
    let mut ed = editor("aaaa bbbb cccc dddd\nx|");
    let map = DisplayMap::for_editor(&ed, Some(10));
    ed.move_rows(&map, -1, false);
    assert_eq!(show(&ed), "aaaa bbbb c|ccc dddd\nx");
    ed.move_rows(&map, -1, false);
    assert_eq!(show(&ed), "a|aaa bbbb cccc dddd\nx");
    ed.move_rows(&map, 2, true);
    assert_eq!(show(&ed), "a[aaa bbbb cccc dddd\nx]");
    // Aiming past the end of a row that wraps: the cursor stays on that
    // row instead of landing at the start of the next.
    let mut ed = editor("aaaa bbbb cccc dddd\nzzzzzzzzzz|");
    let map = DisplayMap::for_editor(&ed, Some(10));
    ed.move_rows(&map, -2, false);
    assert_eq!(show(&ed), "aaaa bbbb| cccc dddd\nzzzzzzzzzz");
}
