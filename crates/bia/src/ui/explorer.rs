//! The sidebar: the workspace's folders and files, one per row.
//!
//! Folders get a chevron and a folder icon; files get an icon for their kind
//! (see `icons.rs`). Icons are tinted with the theme's muted color, and the
//! file open in the editor is tinted with the accent.

use super::{hint, label, style};
use crate::app::{Bia, Message, PROMPT_ID, Prompt, PromptKind, Shelf, ShelfAction};
use crate::icons;
use bia_core::tree;
use iced::widget::{
    Column, button, column, container, mouse_area, row, rule, scrollable, space, svg, text,
    text_input,
};
use iced::{Alignment, Element, Fill, Length, Padding};

const WIDTH: f32 = 248.0;
const INDENT: f32 = 14.0;
const ICON: f32 = 16.0;

pub(super) fn icon<'a>(handle: &svg::Handle, color: iced::Color) -> svg::Svg<'a> {
    svg(handle.clone())
        .width(ICON)
        .height(ICON)
        .style(move |_, _| svg::Style { color: Some(color) })
}

pub fn view(app: &Bia) -> Element<'_, Message> {
    let c = app.look.colors;
    let Some(tree) = &app.tree else { return space().into() };

    let header = row![
        label(app, tree.root_name()).color(c.text),
        space::horizontal(),
        button(text("⋯").size(app.settings.ui_font_size + 2.0))
            .padding([0, 6])
            .style(style::quiet(c))
            .on_press(Message::ToggleShelf),
    ]
    .align_y(Alignment::Center)
    .padding(Padding { top: 0.0, bottom: 0.0, left: 14.0, right: 8.0 })
    .height(Length::Fixed(38.0));

    let open_file = app.tabs.active().map(|t| t.path.as_path());
    let rows = tree.rows().iter().map(|entry| {
        let selected = app.selected.as_deref() == Some(entry.path.as_path());
        let is_open = open_file == Some(entry.path.as_path());
        let (chevron, kind_icon) = match (entry.is_dir, entry.expanded) {
            (true, true) => (Some(icons::get("chevron-down")), icons::get("folder-open")),
            (true, false) => (Some(icons::get("chevron-right")), icons::get("folder")),
            (false, _) => (None, icons::for_file(&entry.name)),
        };
        let chevron: iced::Element<'_, Message> = match chevron {
            Some(handle) => icon(handle, c.muted).into(),
            None => space().width(ICON).into(),
        };
        let content = row![
            space().width(INDENT * entry.depth as f32),
            chevron,
            icon(kind_icon, if is_open { c.accent } else { c.muted }),
            label(app, entry.name.as_str()).wrapping(text::Wrapping::None),
        ]
        .spacing(4)
        .align_y(Alignment::Center);

        let press = button(content)
            .width(Fill)
            .padding(Padding { top: 3.0, bottom: 3.0, left: 8.0, right: 8.0 })
            .style(style::row(c, selected))
            .on_press(Message::RowPressed(entry.path.clone()));
        mouse_area(press).on_right_press(Message::RowContext(entry.path.clone())).into()
    });
    let list: Element<'_, Message> = if tree.rows().is_empty() {
        container(hint(app, app.look.i18n.t("explorer.empty"))).padding([4, 14]).into()
    } else {
        Column::with_children(rows).into()
    };

    let mut sidebar = column![header, scrollable(list).height(Fill).style(style::scroll(c)),];
    if let Some(shelf) = shelf(app) {
        sidebar = sidebar.push(rule::horizontal(1).style(style::rule(c))).push(shelf);
    }

    container(sidebar)
        .width(Length::Fixed(WIDTH))
        .height(Fill)
        .style(style::surface(c.panel, c.text))
        .into()
}

/// The strip under the tree with file actions and their one-line prompts.
fn shelf(app: &Bia) -> Option<Element<'_, Message>> {
    let c = app.look.colors;
    let t = &app.look.i18n;
    let pad = Padding { top: 10.0, bottom: 12.0, left: 14.0, right: 14.0 };

    match &app.shelf {
        Shelf::Hidden => None,
        Shelf::Actions => {
            let root = app.tree.as_ref().map(|tr| tr.root());
            let has_target = app.selected.as_deref().is_some_and(|p| Some(p) != root);
            let action = |key: &str, action: ShelfAction, enabled: bool, danger: bool| {
                let color = match (enabled, danger) {
                    (false, _) => c.border,
                    (true, true) => c.danger,
                    (true, false) => c.text,
                };
                button(label(app, t.t(key).to_owned()).color(color))
                    .width(Fill)
                    .padding([4, 0])
                    .style(style::row(c, false))
                    .on_press_maybe(enabled.then_some(Message::Shelf(action)))
            };
            Some(
                container(
                    column![
                        hint(app, t.t("explorer.actions")),
                        action("shelf.new_file", ShelfAction::NewFile, true, false),
                        action("shelf.new_folder", ShelfAction::NewFolder, true, false),
                        action("shelf.rename", ShelfAction::Rename, has_target, false),
                        action("shelf.delete", ShelfAction::Delete, has_target, true),
                    ]
                    .spacing(2),
                )
                .padding(pad)
                .into(),
            )
        }
        Shelf::Prompt(prompt) => Some(container(prompt_view(app, prompt)).padding(pad).into()),
    }
}

fn prompt_view<'a>(app: &'a Bia, prompt: &'a Prompt) -> Element<'a, Message> {
    let c = app.look.colors;
    let t = &app.look.i18n;
    let relative = |p: &std::path::Path| {
        let root = app.tree.as_ref().map(|tr| tr.root());
        match root.and_then(|r| p.strip_prefix(r).ok()) {
            Some(rel) if rel.as_os_str().is_empty() => tree::display_name(p),
            Some(rel) => rel.display().to_string(),
            None => p.display().to_string(),
        }
    };
    let title = match &prompt.kind {
        PromptKind::NewFile(dir) => t.f("prompt.new_file_in", &[("dir", &relative(dir))]),
        PromptKind::NewFolder(dir) => t.f("prompt.new_folder_in", &[("dir", &relative(dir))]),
        PromptKind::Rename(path) => t.f("prompt.rename", &[("name", &tree::display_name(path))]),
        PromptKind::Delete(path) => t.f("prompt.delete", &[("name", &tree::display_name(path))]),
    };

    let mut content = column![label(app, title)].spacing(8);
    if let PromptKind::Delete(_) = prompt.kind {
        content = content.push(
            row![
                button(label(app, t.t("shelf.delete").to_owned()).color(c.danger))
                    .padding([5, 10])
                    .style(style::outlined(c, false))
                    .on_press(Message::PromptSubmit),
                button(hint(app, t.t("quit.cancel").to_owned()))
                    .padding([5, 10])
                    .style(style::quiet(c))
                    .on_press(Message::Shortcut(crate::keys::Shortcut::Escape)),
            ]
            .spacing(8),
        );
    } else {
        content = content.push(
            text_input("", &prompt.value)
                .id(PROMPT_ID)
                .on_input(Message::PromptInput)
                .on_submit(Message::PromptSubmit)
                .font(app.look.ui_font)
                .size(app.settings.ui_font_size)
                .padding([5, 8])
                .style(style::input(c)),
        );
    }
    content = match &prompt.error {
        Some(error) => content.push(hint(app, error.as_str()).color(c.danger)),
        None => content.push(hint(app, t.t("prompt.hint"))),
    };
    content.into()
}
