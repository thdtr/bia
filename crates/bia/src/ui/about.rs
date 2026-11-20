//! Two cards over the window, "About BIA" and the list of keyboard
//! shortcuts. A click outside them, or Esc, closes them.

use super::{hint, label, logo, style};
use crate::app::{Bia, Message};
use crate::commands::{Command, EDITOR_KEYS};
use crate::keys;
use iced::widget::{
    Column, button, center, column, container, mouse_area, opaque, row, scrollable, space,
};
use iced::{Alignment, Element, Fill, Length};

const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");

/// The system and processor this build is for, the way people write them.
fn platform() -> (&'static str, &'static str) {
    let os = match std::env::consts::OS {
        "linux" => "Linux",
        "macos" => "macOS",
        "windows" => "Windows",
        other => other,
    };
    (os, std::env::consts::ARCH)
}

/// One line that says which BIA this is, for a bug report.
pub fn details() -> String {
    let (os, arch) = platform();
    format!("BIA {} ({os}, {arch})", env!("CARGO_PKG_VERSION"))
}

pub fn about(app: &Bia) -> Element<'_, Message> {
    let c = app.look.colors;
    let t = &app.look.i18n;
    let (os, arch) = platform();
    let action = |key: &str, message: Message| {
        button(label(app, t.t(key).to_owned()))
            .padding([6, 12])
            .style(style::outlined(c, false))
            .on_press(message)
    };
    let content = column![
        logo(app, 48.0),
        space().height(4),
        hint(app, t.t("app.tagline")),
        space().height(12),
        label(app, t.f("about.version", &[("version", &env!("CARGO_PKG_VERSION"))])),
        hint(app, t.f("about.platform", &[("os", &os), ("arch", &arch)])),
        space().height(12),
        hint(app, t.t("app.sealed")).align_x(Alignment::Center),
        hint(app, t.t("about.license")).align_x(Alignment::Center),
        space().height(16),
        row![
            action("about.copy", Message::Copy(details())),
            action("about.repository", Message::Copy(REPOSITORY.to_owned())),
        ]
        .spacing(8),
        action("about.close", Message::CloseOverlay),
    ]
    .spacing(4)
    .align_x(Alignment::Center);
    card(app, container(content).padding(28).width(Length::Fixed(440.0)).into())
}

pub fn shortcuts(app: &Bia) -> Element<'_, Message> {
    let c = app.look.colors;
    let t = &app.look.i18n;
    let line = |name: String, keys: &str| {
        row![label(app, name).width(Fill), hint(app, keys::label(keys))]
            .spacing(16)
            .align_y(Alignment::Center)
    };
    let mut list = Column::new().spacing(6).push(heading(app, t.t("keys.app")));
    for command in Command::ALL {
        if let Some(keys) = app.bindings.keys_for(command) {
            list = list.push(line(t.t(command.label()).to_owned(), &keys));
        }
    }
    list = list.push(space().height(10)).push(heading(app, t.t("keys.editor")));
    for (keys, name) in EDITOR_KEYS {
        list = list.push(line(t.t(name).to_owned(), keys));
    }
    let title = row![
        label(app, t.t("keys.title").to_owned()).size(app.settings.ui_font_size + 3.0),
        space::horizontal(),
        button(label(app, "×").color(c.muted))
            .style(style::quiet(c))
            .on_press(Message::CloseOverlay),
    ]
    .align_y(Alignment::Center);
    let height = (app.window_size().height - 120.0).max(240.0);
    // Room on the right for the scrollbar, so it doesn't cover the keys.
    let body = scrollable(container(list).padding(iced::Padding {
        right: 18.0,
        left: 4.0,
        ..iced::Padding::ZERO
    }))
    .style(style::scroll(c))
    .height(Fill);
    let content = column![title, body].spacing(14);
    card(
        app,
        container(content)
            .padding(24)
            .width(Length::Fixed(560.0))
            .height(Length::Fixed(height))
            .into(),
    )
}

fn heading<'a>(app: &'a Bia, name: &'a str) -> Element<'a, Message> {
    hint(app, name).font(iced::Font { weight: iced::font::Weight::Bold, ..app.look.ui_font }).into()
}

/// A card in the middle of the window, over a dimmed background that
/// closes it when clicked.
fn card<'a>(app: &'a Bia, content: Element<'a, Message>) -> Element<'a, Message> {
    let c = app.look.colors;
    let card = opaque(container(content).style(style::card(c)));
    opaque(mouse_area(center(card).style(style::scrim(c))).on_press(Message::CloseOverlay))
}
