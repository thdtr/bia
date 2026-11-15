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
pub mod layout;
pub mod style;
mod workspace;

use crate::app::{Bia, Message, QuitChoice};
use iced::widget::{
    Text, button, center, column, container, opaque, row, rule, space, stack, text,
};
use iced::{Alignment, Element, Fill, Length, Padding};

pub fn view(app: &Bia) -> Element<'_, Message> {
    let c = app.look.colors;
    let main: Element<'_, Message> = if app.tree.is_none() && app.tabs.is_empty() {
        welcome(app)
    } else {
        let mut main = row![];
        if app.sidebar && app.tree.is_some() {
            main = main.push(explorer::view(app));
            main = main.push(rule::vertical(1).style(style::rule(c)));
        }
        main.push(workspace::view(app)).height(Fill).into()
    };

    let body = column![main, rule::horizontal(1).style(style::rule(c)), status_bar(app)];
    let base = container(body).width(Fill).height(Fill).style(style::surface(c.background, c.text));

    if app.quitting { stack![base, quit_dialog(app)].into() } else { base.into() }
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

    let content = column![
        logo(app, 64.0),
        space().height(6.0),
        hint(app, t.t("app.tagline")),
        space().height(28),
        shortcut_button(t.t("welcome.open_folder"), "Ctrl+O", Message::OpenFolder, true),
        shortcut_button(t.t("welcome.settings"), "Ctrl+,", Message::OpenSettings, false),
        space().height(36),
        hint(app, t.t("app.sealed")),
    ]
    .spacing(10)
    .align_x(Alignment::Center);

    center(content).into()
}

fn status_bar(app: &Bia) -> Element<'_, Message> {
    let c = app.look.colors;
    let t = &app.look.i18n;

    let left: Element<'_, Message> = match (&app.notice, app.tabs.active()) {
        (Some(notice), _) => hint(app, notice.text.as_str())
            .color(if notice.error { c.danger } else { c.text })
            .into(),
        (None, Some(tab)) => {
            let shown = app
                .tree
                .as_ref()
                .and_then(|tree| tab.path.strip_prefix(tree.root()).ok())
                .unwrap_or(&tab.path);
            hint(app, shown.display().to_string()).into()
        }
        (None, None) => space().into(),
    };

    let mut right = row![].spacing(18).align_y(Alignment::Center);
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
        let syntax =
            if buffer.syntax == "Plain Text" { t.t("status.plain_text") } else { &buffer.syntax };
        let encoding = if buffer.bom { "UTF-8 BOM" } else { "UTF-8" };
        right = right
            .push(hint(app, t.f("status.line_col", &[("line", &line), ("col", &col)])))
            .push(hint(app, syntax.to_owned()))
            .push(hint(app, buffer.line_ending.label()))
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

fn quit_dialog(app: &Bia) -> Element<'_, Message> {
    let c = app.look.colors;
    let t = &app.look.i18n;
    let dirty = app.tabs.dirty_indices().len();
    let choice = |key: &str, choice: QuitChoice, emphasis: bool| {
        button(label(app, t.t(key).to_owned()))
            .padding([8, 14])
            .style(style::outlined(c, emphasis))
            .on_press(Message::Quit(choice))
    };
    let card = container(
        column![
            label(app, t.f("quit.message", &[("count", &dirty)])),
            row![
                choice("quit.save_all", QuitChoice::SaveAll, true),
                choice("quit.discard", QuitChoice::Discard, false),
                choice("quit.cancel", QuitChoice::Cancel, false),
            ]
            .spacing(10),
        ]
        .spacing(20),
    )
    .padding(24)
    .style(style::card(c));

    opaque(center(card).style(style::scrim(c)))
}
