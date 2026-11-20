//! Context menus: the list that opens where you right-click.
//!
//! [`on_right_click`] wraps an element and reports where in the window the
//! right click happened, which `mouse_area` doesn't say. It lets the click
//! go on, so a row inside a list can say which row it was, and the list
//! around it (outside any scrolling, where positions are the window's) can
//! open the menu there. Messages arrive in that order: the row's first.
//!
//! The app keeps the open [`Menu`] and [`view`] draws it over everything
//! else, next to the mouse, and closes it on any click outside.

use super::{hint, label, style};
use crate::app::{Bia, Message};
use crate::keys;
use iced::advanced::layout::{self, Layout};
use iced::advanced::widget::{Operation, Tree, Widget};
use iced::advanced::{Clipboard, Shell, mouse, overlay, renderer};
use iced::widget::{Column, button, container, mouse_area, pin, row, rule, space};
use iced::{Alignment, Element, Event, Fill, Length, Point, Rectangle, Size, Theme, Vector};

type Renderer = iced::Renderer;

/// An open context menu.
pub struct Menu {
    /// Where the right click was, in window coordinates.
    pub at: Point,
    pub items: Vec<Item>,
}

pub enum Item {
    Action {
        /// Translation key of its name.
        label: &'static str,
        keys: Option<&'static str>,
        /// `None`: shown, but greyed out.
        message: Option<Message>,
        danger: bool,
    },
    Separator,
}

impl Item {
    pub fn action(label: &'static str, keys: Option<&'static str>, message: Message) -> Self {
        Item::Action { label, keys, message: Some(message), danger: false }
    }

    pub fn danger(label: &'static str, keys: Option<&'static str>, message: Message) -> Self {
        Item::Action { label, keys, message: Some(message), danger: true }
    }
}

const WIDTH: f32 = 320.0;

/// The open menu, over the window. Clicking anywhere else closes it.
pub fn view<'a>(app: &'a Bia, menu: &'a Menu) -> Element<'a, Message> {
    let c = app.look.colors;
    let t = &app.look.i18n;
    let row_height = app.settings.ui_font_size * 1.3 + 10.0;
    let mut list = Column::new().padding(4).width(WIDTH);
    for item in &menu.items {
        list = list.push(match item {
            Item::Separator => {
                Element::from(container(rule::horizontal(1).style(style::rule(c))).padding([4, 6]))
            }
            Item::Action { label: key, keys, message, danger } => {
                let color = match (message.is_some(), danger) {
                    (false, _) => c.border,
                    (true, true) => c.danger,
                    (true, false) => c.text,
                };
                let mut line =
                    row![label(app, t.t(key).to_owned()).color(color), space::horizontal()]
                        .align_y(Alignment::Center);
                if let Some(keys) = keys {
                    line = line.push(hint(app, keys::label(keys)));
                }
                button(line)
                    .width(Fill)
                    .padding([4, 10])
                    .style(style::row(c, false))
                    .on_press_maybe(message.clone())
                    .into()
            }
        });
    }
    let card = container(list).style(style::card(c));

    // Kept inside the window: a menu near the right or bottom edge opens
    // to the left of the mouse, or above it.
    let separators = menu.items.iter().filter(|i| matches!(i, Item::Separator)).count();
    let height =
        (menu.items.len() - separators) as f32 * row_height + separators as f32 * 9.0 + 8.0;
    let window = app.window_size();
    let x = if menu.at.x + WIDTH > window.width { menu.at.x - WIDTH } else { menu.at.x };
    let y = if menu.at.y + height > window.height { menu.at.y - height } else { menu.at.y };

    mouse_area(pin(card).x(x.max(0.0)).y(y.max(0.0)).width(Fill).height(Fill))
        .on_press(Message::CloseMenu)
        .on_right_press(Message::CloseMenu)
        .into()
}

/// Wraps `content` so a right click on it sends `on_click` with where the
/// mouse was. Outside a scrollable, that's in window coordinates.
pub fn on_right_click<'a>(
    content: impl Into<Element<'a, Message>>,
    on_click: impl Fn(Point) -> Message + 'a,
) -> Element<'a, Message> {
    Element::new(RightClick { content: content.into(), on_click: Box::new(on_click) })
}

struct RightClick<'a> {
    content: Element<'a, Message>,
    on_click: Box<dyn Fn(Point) -> Message + 'a>,
}

impl Widget<Message, Theme, Renderer> for RightClick<'_> {
    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn size_hint(&self) -> Size<Length> {
        self.content.as_widget().size_hint()
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content.as_widget_mut().layout(&mut tree.children[0], renderer, limits)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content.as_widget_mut().operate(&mut tree.children[0], layout, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
        if let Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) = event
            && let Some(at) = cursor.position_over(layout.bounds())
        {
            shell.publish((self.on_click)(at));
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a> From<RightClick<'a>> for Element<'a, Message> {
    fn from(widget: RightClick<'a>) -> Self {
        Element::new(widget)
    }
}
