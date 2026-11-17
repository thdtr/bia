//! Tabs, the find bar, the editor and the Markdown preview.

use super::{hint, label, logo, option_toggle, style, tip};
use crate::app::{Bia, EDITOR_ID, FIND_ID, FindOption, Message, REPLACE_ID};
use crate::buffer::Buffer;
use crate::code_view::CodeView;
use crate::keys;
use crate::tabs::{Body, Tab};
use crate::ui::style::Colors;
use iced::widget::{
    Row, button, center, column, container, markdown, mouse_area, rich_text, row, rule, scrollable,
    space, text_input, tooltip,
};
use iced::{Alignment, Background, Border, Element, Fill, FillPortion, Length, Padding, Pixels};
use std::hash::{Hash, Hasher};

pub fn view(app: &Bia) -> Element<'_, Message> {
    let c = app.look.colors;
    let Some(tab) = app.tabs.active() else {
        return container(empty(app)).width(Fill).height(Fill).into();
    };

    let mut col = column![tab_strip(app), rule::horizontal(1).style(style::rule(c))];
    if let (Some(_), Body::Text(_)) = (&app.find, &tab.body) {
        col = col.push(find_bar(app));
    }
    col = col.push(match &tab.body {
        Body::Text(buffer) => editor_area(app, tab, buffer),
        Body::Unsupported(reason) => center(hint(app, reason.as_str())).into(),
    });
    col.width(Fill).height(Fill).into()
}

/// Nothing open: the brand mark and the handful of keys worth knowing.
fn empty(app: &Bia) -> Element<'_, Message> {
    let keys = [
        ("Ctrl+P", "welcome.open_file"),
        ("Ctrl+O", "welcome.open_folder"),
        ("Ctrl+N", "shelf.new_file"),
        ("Ctrl+Shift+F", "search.title"),
        ("Ctrl+,", "welcome.settings"),
    ];
    let mut list = column![].spacing(8);
    for (key, label_key) in keys {
        list = list.push(
            row![
                hint(app, app.look.i18n.t(label_key).to_owned()).width(Length::Fixed(160.0)),
                hint(app, keys::label(key)),
            ]
            .spacing(12),
        );
    }
    center(column![logo(app, 36.0), space().height(22.0), list].align_x(Alignment::Center)).into()
}

fn tab_strip(app: &Bia) -> Element<'_, Message> {
    let c = app.look.colors;
    let tabs = app.tabs.iter().enumerate().map(|(i, tab)| tab_view(app, i, tab));
    let strip = Row::with_children(tabs).height(Length::Fixed(36.0));
    container(
        scrollable(strip)
            .direction(scrollable::Direction::Horizontal(
                scrollable::Scrollbar::default().width(2).scroller_width(2),
            ))
            .style(style::scroll(c)),
    )
    .width(Fill)
    .style(style::surface(c.panel, c.text))
    .into()
}

fn tab_view<'a>(app: &'a Bia, index: usize, tab: &'a Tab) -> Element<'a, Message> {
    let c = app.look.colors;
    let active = index == app.tabs.active_index();
    let dirty = tab.is_dirty();

    let close = button(label(app, if dirty { "●" } else { "×" }).color(if dirty {
        c.accent
    } else {
        c.muted
    }))
    .padding([0, 4])
    .style(style::quiet(c))
    .on_press(Message::CloseTab(index));

    let title = button(label(app, tab.name()))
        .padding(Padding { top: 9.0, bottom: 9.0, left: 14.0, right: 2.0 })
        .style(style::tab(c, active))
        .on_press(Message::SelectTab(index));

    let body = container(
        row![title, close]
            .align_y(Alignment::Center)
            .padding(Padding { right: 8.0, ..Padding::ZERO }),
    )
    .height(Fill)
    .style(style::surface(if active { c.background } else { c.panel }, c.text));

    // The accent marker: a 2px bar under the active tab.
    let marker = container(space())
        .width(Fill)
        .height(Length::Fixed(2.0))
        .style(style::surface(if active { c.accent } else { c.panel }, c.text));

    let tab = column![body, marker].width(Length::Shrink);
    let separator = rule::vertical(1).style(style::rule(c));
    mouse_area(row![tab, separator]).on_middle_press(Message::CloseTab(index)).into()
}

fn find_bar(app: &Bia) -> Element<'_, Message> {
    let c = app.look.colors;
    let t = &app.look.i18n;
    let Some(find) = &app.find else { return space().into() };
    let status = match (find.matches.len(), find.current) {
        _ if find.error.is_some() => t.t("find.invalid").to_owned(),
        (0, _) if find.query.is_empty() => String::new(),
        (0, _) => t.t("find.none").to_owned(),
        (total, Some(i)) => t.f("find.count", &[("current", &(i + 1)), ("total", &total)]),
        (total, None) => t.f("find.count", &[("current", &"–"), ("total", &total)]),
    };
    let field = |placeholder: &str, value: &str, id: &'static str| {
        text_input(placeholder, value)
            .id(id)
            .font(app.look.ui_font)
            .size(app.settings.ui_font_size)
            .padding([4, 8])
            .width(Length::Fixed(260.0))
            .style(style::input(c))
    };
    let status = hint(app, status);
    let status = match &find.error {
        Some(error) => {
            tooltip(status.color(c.danger), tip(app, error.clone()), tooltip::Position::Bottom)
                .into()
        }
        None => Element::from(status),
    };
    let o = find.options;
    let mut rows = column![
        row![
            space::horizontal(),
            status,
            row![
                option_toggle(
                    app,
                    "Aa",
                    "find.case",
                    o.case_sensitive,
                    Message::FindOption(FindOption::Case)
                ),
                option_toggle(
                    app,
                    "ab",
                    "find.word",
                    o.whole_word,
                    Message::FindOption(FindOption::Word)
                ),
                option_toggle(
                    app,
                    ".*",
                    "find.regex",
                    o.regex,
                    Message::FindOption(FindOption::Regex)
                ),
            ]
            .spacing(2),
            field(t.t("find.placeholder"), &find.query, FIND_ID)
                .on_input(Message::FindInput)
                .on_submit(Message::FindSubmit),
        ]
        .spacing(12)
        .align_y(Alignment::Center)
    ]
    .spacing(6);
    if let Some(replace) = &find.replace {
        let action = |key: &str, message: Message| {
            button(hint(app, t.t(key).to_owned()).color(c.text))
                .padding([3, 10])
                .style(style::outlined(c, false))
                .on_press(message)
        };
        rows = rows.push(
            row![
                space::horizontal(),
                action("find.replace", Message::ReplaceOne),
                action("find.replace_all", Message::ReplaceAll),
                field(t.t("find.replace_placeholder"), replace, REPLACE_ID)
                    .on_input(Message::ReplaceInput)
                    .on_submit(Message::ReplaceOne),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        );
    }
    container(rows).padding([6, 12]).width(Fill).style(style::surface(c.background, c.text)).into()
}

fn editor_area<'a>(app: &'a Bia, tab: &'a Tab, buffer: &'a Buffer) -> Element<'a, Message> {
    let c = app.look.colors;
    let (matches, current): (&[std::ops::Range<usize>], _) = match &app.find {
        Some(find) => (&find.matches, find.current),
        None => (&[], None),
    };
    let mut hasher = std::hash::DefaultHasher::new();
    tab.path.hash(&mut hasher);
    let code: Element<'a, Message> = CodeView::new(
        EDITOR_ID,
        &buffer.editor,
        &buffer.highlights,
        &buffer.display,
        app.syntax_context(),
        c,
        Message::Input,
        Message::EditorScrolled,
    )
    .font(app.look.editor_font, app.settings.font_size, app.settings.line_height)
    .matches(matches, current)
    .git(&buffer.git)
    .wrap(app.settings.soft_wrap)
    .scroll(hasher.finish(), tab.scroll, buffer.center)
    .into();

    match (&buffer.markdown, app.is_markdown(tab)) {
        (Some(md), true) => {
            let preview =
                scrollable(container(markdown_view(app, md)).padding([24, 32]).max_width(760.0))
                    .height(Fill)
                    .style(style::scroll(c));
            row![
                container(code).width(FillPortion(1)),
                rule::vertical(1).style(style::rule(c)),
                container(preview).width(FillPortion(1)).height(Fill),
            ]
            .height(Fill)
            .into()
        }
        _ => code,
    }
}

fn markdown_view<'a>(app: &'a Bia, md: &'a markdown::Content) -> Element<'a, Message> {
    let c = app.look.colors;
    let size = app.settings.ui_font_size + 2.0;
    let mut settings = markdown::Settings::with_text_size(
        size,
        markdown::Style {
            font: app.look.ui_font,
            inline_code_highlight: markdown::Highlight {
                background: Background::Color(c.hover),
                border: Border { radius: 3.0.into(), ..Border::default() },
            },
            inline_code_padding: Padding { left: 3.0, right: 3.0, ..Padding::ZERO },
            inline_code_color: c.text,
            inline_code_font: app.look.editor_font,
            code_block_font: app.look.editor_font,
            link_color: c.accent,
        },
    );
    settings.code_size = Pixels(app.settings.font_size * 0.92);
    markdown::view_with(md.items(), settings, &Viewer { colors: c })
}

/// Draws Markdown with BIA's colors instead of iced's defaults.
struct Viewer {
    colors: Colors,
}

impl Viewer {
    /// The rounded, hairline-bordered box used for code blocks and images.
    fn boxed(&self) -> impl Fn(&iced::Theme) -> container::Style + use<> {
        let c = self.colors;
        move |_| container::Style {
            background: Some(Background::Color(c.hover)),
            border: Border { color: c.border, width: 1.0, radius: 6.0.into() },
            ..container::Style::default()
        }
    }
}

impl<'a> markdown::Viewer<'a, Message> for Viewer {
    fn on_link_click(url: markdown::Uri) -> Message {
        Message::LinkClicked(url)
    }

    fn code_block(
        &self,
        settings: markdown::Settings,
        _language: Option<&'a str>,
        _code: &'a str,
        lines: &'a [markdown::Text],
    ) -> Element<'a, Message> {
        // iced stamps the body font on every span, which would override the
        // code font set on the paragraph, so the spans get the code font.
        let style = markdown::Style { font: settings.style.code_block_font, ..settings.style };
        let lines = column(lines.iter().map(|line| {
            rich_text(line.spans(style))
                .on_link_click(Message::LinkClicked)
                .size(settings.code_size)
                .into()
        }));
        container(lines).width(Fill).padding(settings.code_size.0).style(self.boxed()).into()
    }

    /// BIA does not load images (they may be remote): show the alt text.
    fn image(
        &self,
        settings: markdown::Settings,
        _url: &'a markdown::Uri,
        _title: &'a str,
        alt: &markdown::Text,
    ) -> Element<'a, Message> {
        let muted = self.colors.muted;
        let spans: Vec<_> =
            alt.spans(settings.style).iter().cloned().map(|s| s.color(muted)).collect();
        container(rich_text(spans).size(settings.text_size * 0.85))
            .padding([2, 8])
            .style(self.boxed())
            .into()
    }
}
