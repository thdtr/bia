//! The terminal panel under the editor, and the dividers you drag to
//! resize the panels.
//!
//! ```text
//! ├─────────────────────────────────────────────┤ ← drag to resize
//! │ zsh ×   cargo ×   +                       ⌄ │
//! ├─────────────────────────────────────────────┤
//! │ $ _                                         │
//! ```

use super::explorer::icon;
use super::{hint, style, tip};
use crate::app::{Bia, Message, Splitter, TERMINAL_ID};
use crate::icons;
use crate::keys;
use crate::terminal::Terminal;
use iced::widget::{Row, button, column, container, mouse_area, row, rule, space, tooltip};
use iced::{Alignment, Element, Fill, Length, Padding, mouse};

/// The strip you grab to resize. It is wider than the line it draws, and
/// painted like the panel it belongs to, so it's easy to hit.
const GRIP: f32 = 4.0;
const HEADER: f32 = 34.0;

pub fn terminal(app: &Bia) -> Option<Element<'_, Message>> {
    let c = app.look.colors;
    if !app.terminals.open {
        return None;
    }
    let active = app.terminals.active()?;
    let view = crate::terminal_view::TerminalView::new(
        TERMINAL_ID,
        active,
        app.look.terminal_font,
        app.settings.terminal.font_size.unwrap_or(app.settings.font_size),
        c,
        |size| Message::TermResize(active.id, size),
        |location| Message::TermOpen(active.id, location),
    );

    let tabs = app.terminals.list.iter().map(|t| tab(app, t, t.id == active.id));
    let header = row![
        Row::with_children(tabs).spacing(2).align_y(Alignment::Center),
        icon_button(app, "plus", "terminal.new", Some("Ctrl+Shift+`"), Message::NewTerminal, false),
        space::horizontal(),
        icon_button(
            app,
            "chevron-down",
            "terminal.hide",
            Some("Ctrl+`"),
            Message::ToggleTerminal,
            false
        ),
    ]
    .spacing(4)
    .align_y(Alignment::Center)
    .padding(Padding { top: 0.0, bottom: 0.0, left: 8.0, right: 8.0 })
    .height(Length::Fixed(HEADER));

    // The empty part of the header is a grip too.
    let top = mouse_area(column![
        rule::horizontal(1).style(style::rule(c)),
        container(space()).width(Fill).height(GRIP),
        header,
    ])
    .interaction(mouse::Interaction::ResizingVertically)
    .on_press(Message::DragStart(Splitter::Terminal));

    let panel = column![
        container(top).width(Fill).style(style::surface(c.panel, c.text)),
        rule::horizontal(1).style(style::rule(c)),
        container(view).height(Fill),
    ];
    Some(container(panel).height(Length::Fixed(app.terminal_height())).into())
}

/// One terminal's tab, with its title and × to close it.
fn tab<'a>(app: &'a Bia, terminal: &'a Terminal, active: bool) -> Element<'a, Message> {
    let c = app.look.colors;
    let title: String = terminal.name().chars().take(28).collect();
    let name = hint(app, title).color(if active { c.text } else { c.muted });
    let close = button(icon(icons::get("close"), c.muted))
        .padding(4)
        .style(style::quiet(c))
        .on_press(Message::CloseTerminal(terminal.id));
    button(row![name, close].spacing(6).align_y(Alignment::Center))
        .padding(Padding { top: 3.0, bottom: 3.0, left: 10.0, right: 4.0 })
        .style(style::outlined(c, active))
        .on_press(Message::SelectTerminal(terminal.id))
        .into()
}

/// Space around a 16 px icon, so the button is 28 px wide and tall: a
/// comfortable target for a mouse, as desktop guidelines ask (24 px at
/// the least).
pub(super) const ICON_PAD: f32 = 6.0;

/// Room between a button and its tooltip, so the tooltip never covers it.
pub(super) const TIP_GAP: f32 = 6.0;

/// An icon button with its name, and its keys if it has some, on hover.
/// `below` puts the tooltip under the button, for buttons at the top of
/// the window, where there's no room above.
pub(super) fn icon_button<'a>(
    app: &'a Bia,
    name: &str,
    label: &str,
    keys: Option<&str>,
    message: Message,
    below: bool,
) -> Element<'a, Message> {
    let c = app.look.colors;
    let face = button(icon(icons::get(name), c.muted)).padding(ICON_PAD).style(style::quiet(c));
    let mut text = app.look.i18n.t(label).to_owned();
    if let Some(keys) = keys {
        text = format!("{text}   {}", keys::label(keys));
    }
    let side = if below { tooltip::Position::Bottom } else { tooltip::Position::Top };
    tooltip(face.on_press(message), tip(app, text), side).gap(TIP_GAP).into()
}

/// The divider between the sidebar and the editor.
pub fn sidebar_grip(app: &Bia) -> Element<'_, Message> {
    let c = app.look.colors;
    let grip = container(space()).width(GRIP).height(Fill).style(style::surface(c.panel, c.text));
    let grip = mouse_area(grip)
        .interaction(mouse::Interaction::ResizingHorizontally)
        .on_press(Message::DragStart(Splitter::Sidebar));
    row![grip, rule::vertical(1).style(style::rule(c))].height(Fill).into()
}
