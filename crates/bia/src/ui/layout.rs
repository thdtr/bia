//! Editor geometry: the numbers the view lays out with and the update loop
//! scrolls with. Both read these numbers, so they always match.

use iced::Padding;

/// Space around the text inside the editor's scroll area.
pub const EDITOR_PADDING: Padding = Padding { top: 14.0, right: 32.0, bottom: 48.0, left: 14.0 };

/// Space left and right of the numbers in the line-number gutter.
pub const GUTTER_PADDING: (f32, f32) = (20.0, 6.0);

/// Files longer than this open without the gutter: iced then lays out only
/// the visible lines and scrolls by itself, which keeps huge files usable.
pub const LARGE_FILE_LINES: usize = 10_000;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Geometry {
    /// Height of one line, in pixels.
    pub line: f32,
    /// Advance of one character of the editor font, in pixels.
    pub char_width: f32,
}

impl Geometry {
    pub fn new(font_size: f32, line_height: f32, char_width: f32) -> Self {
        Self { line: font_size * line_height, char_width }
    }

    /// Width of the gutter for a file with `line_count` lines (at least
    /// three digits wide, so it doesn't jump while you type).
    pub fn gutter_width(&self, line_count: usize) -> f32 {
        let digits = line_count.to_string().len().max(3) as f32;
        GUTTER_PADDING.0 + GUTTER_PADDING.1 + digits * self.char_width
    }

    /// Width the editor needs so its widest line fits without wrapping.
    pub fn text_width(&self, max_columns: usize) -> f32 {
        EDITOR_PADDING.left + EDITOR_PADDING.right + (max_columns + 2) as f32 * self.char_width
    }

    /// The smallest scroll that brings the cursor into view, per axis.
    /// `None` on an axis means it is already visible there.
    ///
    /// `cursor` is (line, column in monospace cells); `offset` and `size`
    /// describe the visible part of the scroll area.
    pub fn reveal(
        &self,
        cursor: (usize, usize),
        line_count: usize,
        offset: (f32, f32),
        size: (f32, f32),
    ) -> (Option<f32>, Option<f32>) {
        let y = EDITOR_PADDING.top + cursor.0 as f32 * self.line;
        let x =
            self.gutter_width(line_count) + EDITOR_PADDING.left + cursor.1 as f32 * self.char_width;

        // Keep one line of context above and below the cursor.
        let new_y = if y - self.line < offset.1 {
            Some((y - self.line).max(0.0))
        } else if y + 2.0 * self.line > offset.1 + size.1 {
            Some(y + 2.0 * self.line - size.1)
        } else {
            None
        };
        let margin = 4.0 * self.char_width;
        let new_x = if x + margin > offset.0 + size.0 {
            Some(x + margin - size.0)
        } else if offset.0 > 0.0 && x - margin < offset.0 {
            Some((x - size.0 / 3.0).max(0.0))
        } else {
            None
        };
        (new_x, new_y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const G: Geometry = Geometry { line: 20.0, char_width: 8.0 };

    #[test]
    fn visible_cursor_needs_no_scroll() {
        assert_eq!(G.reveal((5, 10), 100, (0.0, 0.0), (800.0, 600.0)), (None, None));
    }

    #[test]
    fn scrolls_down_and_up_with_context() {
        let (_, y) = G.reveal((50, 0), 100, (0.0, 0.0), (800.0, 600.0));
        // Cursor line 50 at y = 14 + 1000; one extra line below it stays visible.
        assert_eq!(y, Some(14.0 + 1000.0 + 40.0 - 600.0));
        let (_, y) = G.reveal((2, 0), 100, (0.0, 500.0), (800.0, 600.0));
        assert_eq!(y, Some(14.0 + 40.0 - 20.0));
    }

    #[test]
    fn scrolls_sideways_only_when_needed() {
        let (x, _) = G.reveal((0, 200), 100, (0.0, 0.0), (800.0, 600.0));
        assert!(x.is_some_and(|x| x > 0.0));
        // Back at column 0 after scrolling right: scroll back.
        let (x, _) = G.reveal((0, 0), 100, (900.0, 0.0), (800.0, 600.0));
        assert_eq!(x, Some(0.0));
    }

    #[test]
    fn gutter_grows_with_digits() {
        assert_eq!(G.gutter_width(9), G.gutter_width(999));
        assert!(G.gutter_width(10_000) > G.gutter_width(999));
    }
}
