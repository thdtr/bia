//! Turns font names from `settings.json` into iced fonts.

use iced::advanced::text::{Paragraph as _, Shaping, Text};
use iced::alignment::Vertical;
use iced::font::Family;
use iced::widget::text::{Alignment, LineHeight, Wrapping};
use iced::{Font, Pixels, Size};
use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

/// iced wants `&'static str` family names. Names are interned so changing
/// fonts back and forth never leaks more than one copy of each name.
fn intern(name: &str) -> &'static str {
    static NAMES: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();
    let mut names = NAMES.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
    if let Some(existing) = names.get(name) {
        return existing;
    }
    let leaked: &'static str = Box::leak(name.to_owned().into_boxed_str());
    names.insert(leaked);
    leaked
}

/// BIA ships its own fonts so it looks the same on every machine:
/// Fira Sans for the interface and Fira Code for text. Both are SIL Open
/// Font License (see `assets/fonts/`).
pub const UI_FAMILY: &str = "Fira Sans";

const BUNDLED: &[&[u8]] = &[
    include_bytes!("../../../assets/fonts/FiraSans-Regular.ttf"),
    include_bytes!("../../../assets/fonts/FiraSans-Italic.ttf"),
    include_bytes!("../../../assets/fonts/FiraSans-Bold.ttf"),
    include_bytes!("../../../assets/fonts/FiraSans-BoldItalic.ttf"),
    include_bytes!("../../../assets/fonts/FiraCode-Regular.ttf"),
    include_bytes!("../../../assets/fonts/FiraCode-Bold.ttf"),
];

/// Registers the bundled fonts with iced's text engine. Called once, before
/// anything is measured or drawn.
pub fn register_bundled() {
    let mut system = iced::advanced::graphics::text::font_system()
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    for bytes in BUNDLED {
        system.load_font(std::borrow::Cow::Borrowed(bytes));
    }
}

pub fn from_setting(name: &str) -> Font {
    let name = name.trim();
    match name.to_ascii_lowercase().as_str() {
        "monospace" | "mono" => Font::MONOSPACE,
        "sans-serif" | "sans" | "" => Font::DEFAULT,
        "serif" => Font { family: Family::Serif, ..Font::DEFAULT },
        _ => Font::with_name(intern(name)),
    }
}

/// Bounds for a paragraph that will be drawn: `width` (or infinite) and a
/// height derived from `key`, a hash of what the paragraph shows.
///
/// iced 0.14 tells paragraphs apart by font, size and bounds, not by their
/// text, and its CPU renderer skips redrawing what it thinks is equal: a
/// row whose `1` became a `2` stayed a `1` on screen. The height never
/// matters for one line of top-aligned text, so it carries the difference.
pub fn tagged_bounds(width: f32, key: u64) -> Size {
    // Whole numbers up to 2^24 are exact in an f32.
    Size::new(width, 4_000_000.0 + (key % 8_000_000) as f32)
}

/// Advance width of one character in `font`, measured with the same text
/// engine the editor uses. Exact for monospace fonts, an average otherwise.
pub fn char_width(font: Font, size: f32, line_height: f32) -> f32 {
    type Paragraph = <iced::Renderer as iced::advanced::text::Renderer>::Paragraph;
    const SAMPLE: &str = "0123456789abcdefghijklmnopqrstuvwxyz";
    let paragraph = Paragraph::with_text(Text {
        content: SAMPLE,
        bounds: Size::INFINITE,
        size: Pixels(size),
        line_height: LineHeight::Relative(line_height),
        font,
        align_x: Alignment::Default,
        align_y: Vertical::Top,
        shaping: Shaping::Advanced,
        wrapping: Wrapping::None,
    });
    let width = paragraph.min_width() / SAMPLE.len() as f32;
    if width.is_finite() && width > 0.0 { width } else { size * 0.6 }
}
