//! The view: a pure function from [`Bia`] to widgets.
//!
//! ```text
//! ┌──────────┬───────────────────────────────────────────┐
//! │ explorer │ tabs                                      │
//! │          ├───────────────────────────────────────────┤
//! │          │ editor                    │ md preview    │
//! │          │                           │ (optional)    │
//! │ (shelf)  │                                           │
//! ├──────────┴───────────────────────────────────────────┤
//! │ status                                               │
//! └──────────────────────────────────────────────────────┘
//! ```

mod explorer;
mod palette;
mod search;
pub mod style;
mod workspace;

use crate::app::{Bia, Choice, Dialog, Message, Panel};
use iced::widget::{
    Text, button, center, column, container, opaque, row, rule, space, stack, text, tooltip,
};
use iced::{Alignment, Element, Fill, Length, Padding};

/// Height of the terminal panel.
const TERMINAL_HEIGHT: f32 = 260.0;

pub fn view(app: &Bia) -> Element<'_, Message> {
    let c = app.look.colors;
    let main: Element<'_, Message> = if app.tree.is_none() && app.tabs.is_empty() {
        welcome(app)
    } else {
        let mut main = row![];
        if app.sidebar && app.tree.is_some() {
            main = main.push(match app.panel {
                Panel::Files => explorer::view(app),
                Panel::Search => search::view(app),
            });
            main = main.push(rule::vertical(1).style(style::rule(c)));
        }
        main.push(workspace::view(app)).height(Fill).into()
    };

    let mut body = column![main];
    if app.terminal_open
        && let Some(terminal) = &app.terminal
    {
        let view = crate::terminal_view::TerminalView::new(
            crate::app::TERMINAL_ID,
            terminal,
            app.look.editor_font,
            app.settings.font_size,
            c,
            Message::TermResize,
        );
        body = body
            .push(rule::horizontal(1).style(style::rule(c)))
            .push(container(view).height(Length::Fixed(TERMINAL_HEIGHT)));
    }
    let body = body.push(rule::horizontal(1).style(style::rule(c))).push(status_bar(app));
    let base = container(body).width(Fill).height(Fill).style(style::surface(c.background, c.text));

    let mut layers = stack![base];
    if let Some(p) = &app.palette {
        layers = layers.push(palette::view(app, p));
    }
    if let Some(dialog) = &app.dialog {
        layers = layers.push(dialog_view(app, dialog));
    }
    layers.into()
}

/// Interface text in the UI font.
pub(crate) fn label<'a>(app: &Bia, content: impl text::IntoFragment<'a>) -> Text<'a> {
    text(content).font(app.look.ui_font).size(app.settings.ui_font_size)
}

/// Smaller, muted interface text.
pub(crate) fn hint<'a>(app: &Bia, content: impl text::IntoFragment<'a>) -> Text<'a> {
    text(content)
        .font(app.look.ui_font)
        .size(app.settings.ui_font_size - 1.0)
        .color(app.look.colors.muted)
}

/// A search option (match case, whole word, regex): a symbol lit when on,
/// with its name on hover.
pub(crate) fn option_toggle<'a>(
    app: &'a Bia,
    symbol: &'a str,
    name: &str,
    on: bool,
    message: Message,
) -> Element<'a, Message> {
    let c = app.look.colors;
    let face = text(symbol)
        .font(app.look.editor_font)
        .size(app.settings.ui_font_size - 1.0)
        .color(if on { c.accent } else { c.muted });
    let button = button(face).padding([3, 6]).style(style::outlined(c, on)).on_press(message);
    tooltip(button, tip(app, app.look.i18n.t(name).to_owned()), tooltip::Position::Bottom).into()
}

/// The small card a tooltip shows.
pub(crate) fn tip(app: &Bia, content: String) -> Element<'_, Message> {
    container(hint(app, content).color(app.look.colors.text))
        .padding([4, 8])
        .style(style::card(app.look.colors))
        .into()
}

/// The logo at the given height (`assets/logo.svg`, in theme colors).
pub(crate) fn logo(app: &Bia, height: f32) -> Element<'_, Message> {
    // The logo's viewBox is about 2.7 times wider than it is tall.
    iced::widget::svg(app.look.logo.clone()).height(height).width(height * 2.7).into()
}

fn welcome(app: &Bia) -> Element<'_, Message> {
    let c = app.look.colors;
    let t = &app.look.i18n;
    let shortcut_button = |name: &str, keys: &str, message: Message, emphasis: bool| {
        button(
            row![
                label(app, name.to_owned()),
                space::horizontal(),
                hint(app, crate::keys::label(keys))
            ]
            .spacing(24)
            .align_y(Alignment::Center),
        )
        .width(260)
        .padding([9, 14])
        .style(style::outlined(c, emphasis))
        .on_press(message)
    };

    let mut content = column![
        logo(app, 64.0),
        space().height(6.0),
        hint(app, t.t("app.tagline")),
        space().height(28),
        shortcut_button(t.t("welcome.open_folder"), "Ctrl+O", Message::OpenFolder, true),
        shortcut_button(t.t("welcome.settings"), "Ctrl+,", Message::OpenSettings, false),
    ]
    .spacing(10)
    .align_x(Alignment::Center);

    if !app.recent.is_empty() {
        let mut recent = column![hint(app, t.t("welcome.recent"))].spacing(2).width(260);
        for folder in app.recent.iter().take(5) {
            let parent = folder.parent().map(|p| p.display().to_string()).unwrap_or_default();
            let name = bia_core::tree::display_name(folder);
            recent = recent.push(
                button(
                    row![
                        label(app, name).wrapping(text::Wrapping::None),
                        hint(app, parent).wrapping(text::Wrapping::None),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                )
                .width(Fill)
                .padding([4, 6])
                .style(style::row(c, false))
                .on_press(Message::OpenRecent(folder.clone())),
            );
        }
        content = content.push(space().height(18)).push(recent);
    }
    content = content.push(space().height(36)).push(hint(app, t.t("app.sealed")));

    center(content).into()
}

fn status_bar(app: &Bia) -> Element<'_, Message> {
    let c = app.look.colors;
    let t = &app.look.i18n;

    // The problem under the cursor, if a language server found one.
    let problem = app.tabs.active().and_then(|tab| {
        let buffer = tab.buffer()?;
        let cursor = buffer.cursor_offset();
        app.lsp
            .problems(&tab.path, buffer.editor.rope())
            .into_iter()
            .find(|(r, ..)| r.start <= cursor && cursor <= r.end)
            .map(|(_, severity, message)| (severity, message.to_owned()))
    });
    let left: Element<'_, Message> = match (&app.notice, app.tabs.active(), problem) {
        (Some(notice), ..) => hint(app, notice.text.as_str())
            .color(if notice.error { c.danger } else { c.text })
            .into(),
        (None, Some(_), Some((severity, message))) => {
            let color = match severity {
                bia_core::lsp::types::Severity::Error => c.danger,
                bia_core::lsp::types::Severity::Warning => c.modified,
                _ => c.muted,
            };
            let first = message.lines().next().unwrap_or_default().to_owned();
            hint(app, first).color(color).wrapping(text::Wrapping::None).into()
        }
        (None, Some(tab), None) => {
            let shown = app
                .tree
                .as_ref()
                .and_then(|tree| tab.path.strip_prefix(tree.root()).ok())
                .unwrap_or(&tab.path);
            hint(app, shown.display().to_string()).into()
        }
        (None, None, _) => space().into(),
    };

    let mut right = row![].spacing(18).align_y(Alignment::Center);
    if let Some(tab) = app.tabs.active() {
        let (errors, warnings) = app.lsp.counts(&tab.path);
        if errors + warnings > 0 {
            let counts = t.f("status.problems", &[("errors", &errors), ("warnings", &warnings)]);
            let color = if errors > 0 { c.danger } else { c.modified };
            right = right.push(hint(app, counts).color(color));
        }
    }
    right = right.push(
        button(hint(app, t.t("status.terminal")).color(if app.terminal_open {
            c.accent
        } else {
            c.muted
        }))
        .padding(0)
        .style(style::quiet(c))
        .on_press(Message::ToggleTerminal),
    );
    if let Some(branch) = &app.git.branch {
        let icon = explorer::icon(crate::icons::get("git"), c.muted);
        right = right
            .push(row![icon, hint(app, branch.as_str())].spacing(4).align_y(Alignment::Center));
    }
    if let Some(tab) = app.tabs.active()
        && let Some(buffer) = tab.buffer()
    {
        if app.is_markdown(tab) {
            let on = buffer.markdown.is_some();
            right = right.push(
                button(hint(app, t.t("status.preview")).color(if on { c.accent } else { c.muted }))
                    .padding(0)
                    .style(style::quiet(c))
                    .on_press(Message::TogglePreview),
            );
        }
        let (line, col) = buffer.line_col();
        let syntax = if buffer.editor.syntax == "Plain Text" {
            t.t("status.plain_text")
        } else {
            &buffer.editor.syntax
        };
        let encoding = if buffer.bom { "UTF-8 BOM" } else { "UTF-8" };
        let indent = if buffer.editor.indent.tabs {
            t.t("status.tabs").to_owned()
        } else {
            t.f("status.spaces", &[("width", &buffer.editor.indent.width)])
        };
        let eol = buffer.line_ending.label();
        let eol = if buffer.mixed_endings {
            t.f("status.mixed_eol", &[("eol", &eol)])
        } else {
            eol.into()
        };
        right = right
            .push(hint(app, t.f("status.line_col", &[("line", &line), ("col", &col)])))
            .push(hint(app, indent))
            .push(hint(app, syntax.to_owned()))
            .push(hint(app, eol))
            .push(hint(app, encoding));
    }

    container(row![left, space::horizontal(), right].align_y(Alignment::Center))
        .padding(Padding { top: 0.0, bottom: 0.0, left: 14.0, right: 14.0 })
        .height(Length::Fixed(26.0))
        .align_y(Alignment::Center)
        .width(Fill)
        .style(style::surface(c.panel, c.muted))
        .into()
}

fn dialog_view<'a>(app: &'a Bia, dialog: &Dialog) -> Element<'a, Message> {
    let c = app.look.colors;
    let t = &app.look.i18n;
    let name = |path: &std::path::Path| bia_core::tree::display_name(path);
    // (message, labels for Save / Discard, which choice is the safe default)
    let (message, save, discard, safe) = match dialog {
        Dialog::Quit => {
            let dirty = app.tabs.dirty_indices().len();
            (
                t.f("quit.message", &[("count", &dirty)]),
                "quit.save_all",
                "quit.discard",
                Choice::Save,
            )
        }
        Dialog::Close(path) => (
            t.f("close.message", &[("name", &name(path))]),
            "close.save",
            "close.discard",
            Choice::Save,
        ),
        Dialog::Conflict(path) => (
            t.f("conflict.message", &[("name", &name(path))]),
            "conflict.overwrite",
            "conflict.reload",
            Choice::Cancel,
        ),
    };
    let choice = |key: &str, choice: Choice| {
        button(label(app, t.t(key).to_owned()))
            .padding([8, 14])
            .style(style::outlined(c, choice == safe))
            .on_press(Message::Dialog(choice))
    };
    let card = container(
        column![
            label(app, message),
            row![
                choice(save, Choice::Save),
                choice(discard, Choice::Discard),
                choice("quit.cancel", Choice::Cancel),
            ]
            .spacing(10),
        ]
        .spacing(20),
    )
    .padding(24)
    .style(style::card(c));

    opaque(center(card).style(style::scrim(c)))
}
