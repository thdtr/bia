//! The box at the top of the window. It opens a file by name (Ctrl+P), goes to
//! a line (Ctrl+G), run a command (Ctrl+Shift+P) or open a recent folder.
//! Clicking outside it, or Esc, closes it.

use super::explorer::icon;
use super::{hint, label, style};
use crate::app::{Bia, Message, PALETTE_ID, Palette, PaletteMode};
use crate::icons;
use crate::tabs::Tab;
use iced::widget::{
    Column, button, column, container, mouse_area, opaque, row, scrollable, space, text, text_input,
};
use iced::{Alignment, Element, Fill, Length, Padding};

const WIDTH: f32 = 560.0;

pub fn view<'a>(app: &'a Bia, palette: &'a Palette) -> Element<'a, Message> {
    let c = app.look.colors;
    let t = &app.look.i18n;

    let placeholder = match palette.mode {
        PaletteMode::Files { .. } => t.t("palette.files"),
        PaletteMode::Line => t.t("palette.line"),
        PaletteMode::Rename { .. } => t.t("palette.rename"),
        PaletteMode::Commands { .. } => t.t("palette.commands"),
        PaletteMode::Recent { .. } => t.t("palette.recent"),
        PaletteMode::History { .. } => t.t("palette.history"),
    };
    let input = text_input(placeholder, &palette.query)
        .id(PALETTE_ID)
        .on_input(Message::PaletteInput)
        .on_submit(Message::PaletteSubmit)
        .font(app.look.ui_font)
        .size(app.settings.ui_font_size + 1.0)
        .padding([7, 10])
        .style(style::input(c));

    let below: Element<'a, Message> = match &palette.mode {
        PaletteMode::Files { files, results } => {
            if results.is_empty() {
                let note = match files {
                    None => t.t("palette.listing").to_owned(),
                    Some(list) if palette.query.trim().is_empty() => {
                        t.f("palette.count", &[("count", &list.files.len())])
                    }
                    Some(_) => t.t("find.none").to_owned(),
                };
                note_row(app, note)
            } else {
                let rows = results.iter().enumerate().map(|(i, path)| {
                    let (dir, name) = path.rsplit_once('/').unwrap_or(("", path));
                    let content = row![
                        icon(icons::for_file(name), c.muted),
                        label(app, name).wrapping(text::Wrapping::None),
                        hint(app, dir).wrapping(text::Wrapping::None),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center);
                    button(content)
                        .width(Fill)
                        .padding([4, 8])
                        .style(style::row(c, i == palette.selected))
                        .on_press(Message::PalettePick(i))
                        .into()
                });
                let mut list = Column::with_children(rows);
                if let Some(files) = files.as_ref().filter(|f| f.truncated) {
                    let note = t.f("palette.truncated", &[("count", &files.files.len())]);
                    list = list.push(note_row(app, note));
                }
                list.into()
            }
        }
        PaletteMode::Rename { .. } => note_row(app, t.t("palette.rename_hint").to_owned()),
        PaletteMode::Commands { results } => {
            let rows = results.iter().enumerate().map(|(i, command)| {
                let mut line =
                    row![label(app, t.t(command.label()).to_owned()), space::horizontal()]
                        .align_y(Alignment::Center);
                if let Some(keys) = command.keys() {
                    line = line.push(hint(app, crate::keys::label(keys)));
                }
                button(line)
                    .width(Fill)
                    .padding([4, 8])
                    .style(style::row(c, i == palette.selected))
                    .on_press(Message::PalettePick(i))
                    .into()
            });
            if results.is_empty() {
                note_row(app, t.t("find.none").to_owned())
            } else {
                scrollable(Column::with_children(rows))
                    .height(Length::Shrink)
                    .style(style::scroll(c))
                    .into()
            }
        }
        PaletteMode::History { versions, .. } => {
            if versions.is_empty() {
                note_row(app, t.t("palette.no_history").to_owned())
            } else {
                let rows = versions.iter().enumerate().take(12).map(|(i, version)| {
                    button(label(app, app.ago(version.saved)))
                        .width(Fill)
                        .padding([4, 8])
                        .style(style::row(c, i == palette.selected))
                        .on_press(Message::PalettePick(i))
                        .into()
                });
                Column::with_children(rows).into()
            }
        }
        PaletteMode::Recent { results } => {
            if results.is_empty() {
                note_row(app, t.t("palette.no_recent").to_owned())
            } else {
                let rows = results.iter().enumerate().map(|(i, folder)| {
                    let parent =
                        folder.parent().map(|p| p.display().to_string()).unwrap_or_default();
                    let content = row![
                        icon(icons::get("folder"), c.muted),
                        label(app, bia_core::tree::display_name(folder))
                            .wrapping(text::Wrapping::None),
                        hint(app, parent).wrapping(text::Wrapping::None),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center);
                    button(content)
                        .width(Fill)
                        .padding([4, 8])
                        .style(style::row(c, i == palette.selected))
                        .on_press(Message::PalettePick(i))
                        .into()
                });
                Column::with_children(rows).into()
            }
        }
        PaletteMode::Line => {
            let note = match app.tabs.active().and_then(Tab::buffer) {
                Some(buffer) => t.f(
                    "palette.line_hint",
                    &[("line", &buffer.line_col().0), ("total", &buffer.editor.line_count())],
                ),
                None => String::new(),
            };
            note_row(app, note)
        }
    };

    let card = container(column![input, below].spacing(6))
        .padding(8)
        .width(Length::Fixed(WIDTH))
        .style(style::card(c));
    let layer = container(opaque(card))
        .width(Fill)
        .height(Fill)
        .align_x(Alignment::Center)
        .padding(Padding { top: 56.0, ..Padding::ZERO });
    opaque(mouse_area(layer).on_press(Message::PaletteClose))
}

fn note_row(app: &Bia, note: String) -> Element<'_, Message> {
    container(hint(app, note)).padding([4, 8]).into()
}
