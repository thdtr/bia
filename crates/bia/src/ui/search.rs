//! Find in files (Ctrl+Shift+F): the sidebar's second panel. The query runs
//! when you press Enter; each result opens its file with the match selected.

use super::explorer::icon;
use super::{hint, label, option_toggle, style};
use crate::app::{Bia, FindOption, Message, Panel, SEARCH_ID};
use crate::icons;
use iced::widget::text::{Span, Wrapping};
use iced::widget::{
    Column, button, column, container, rich_text, row, scrollable, space, span, text, text_input,
};
use iced::{Alignment, Element, Fill, Font, Length, Padding};

pub fn view(app: &Bia) -> Element<'_, Message> {
    let c = app.look.colors;
    let t = &app.look.i18n;
    let search = &app.project_search;

    let header = row![
        label(app, t.t("search.title")).color(c.text),
        space::horizontal(),
        button(text("×").size(app.settings.ui_font_size + 2.0))
            .padding([0, 6])
            .style(style::quiet(c))
            .on_press(Message::ShowPanel(Panel::Files)),
    ]
    .align_y(Alignment::Center)
    .padding(Padding { top: 0.0, bottom: 0.0, left: 14.0, right: 8.0 })
    .height(Length::Fixed(38.0));

    let input = text_input(t.t("search.placeholder"), &search.query)
        .id(SEARCH_ID)
        .on_input(Message::SearchInput)
        .on_submit(Message::SearchSubmit)
        .font(app.look.ui_font)
        .size(app.settings.ui_font_size)
        .padding([5, 8])
        .style(style::input(c));
    let o = search.options;
    let toggle = |symbol, name, on, option| {
        option_toggle(app, symbol, name, on, Message::SearchOption(option))
    };
    let options = row![
        toggle("Aa", "find.case", o.case_sensitive, FindOption::Case),
        toggle("ab", "find.word", o.whole_word, FindOption::Word),
        toggle(".*", "find.regex", o.regex, FindOption::Regex),
    ]
    .spacing(2);

    let status = match (&search.error, search.running, &search.found) {
        (Some(e), _, _) => hint(app, format!("{}: {e}", t.t("find.invalid"))).color(c.danger),
        (None, true, _) => hint(app, t.t("search.running")),
        (None, false, Some(found)) if found.hits == 0 => hint(app, t.t("find.none")),
        (None, false, Some(found)) => {
            let key = match (found.truncated, found.files.len(), found.hits) {
                (true, ..) => "search.count_truncated",
                (false, _, 1) => "search.count_one",
                (false, 1, _) => "search.count_one_file",
                _ => "search.count",
            };
            hint(app, t.f(key, &[("hits", &found.hits), ("files", &found.files.len())]))
        }
        (None, false, None) => hint(app, t.t("search.hint")),
    };

    let controls = column![input, options, status.wrapping(Wrapping::WordOrGlyph)]
        .spacing(8)
        .padding(Padding { top: 0.0, bottom: 10.0, left: 14.0, right: 14.0 });

    let mut list = Column::new();
    if let Some(found) = &search.found {
        let root = app.tree.as_ref().map(|t| t.root());
        for (f, file) in found.files.iter().enumerate() {
            let relative = root
                .and_then(|r| file.path.strip_prefix(r).ok())
                .unwrap_or(&file.path)
                .to_string_lossy()
                .replace('\\', "/");
            let (dir, name) = relative.rsplit_once('/').unwrap_or(("", &relative));
            let title = row![
                icon(icons::for_file(name), c.muted),
                label(app, name.to_owned()).wrapping(Wrapping::None),
                hint(app, dir.to_owned()).wrapping(Wrapping::None),
            ]
            .spacing(6)
            .align_y(Alignment::Center);
            list = list.push(
                button(title)
                    .width(Fill)
                    .padding(Padding { top: 6.0, bottom: 2.0, left: 10.0, right: 8.0 })
                    .style(style::row(c, false))
                    .on_press(Message::SearchOpen(f, 0)),
            );
            for (h, hit) in file.hits.iter().enumerate() {
                let p = &hit.preview;
                let (a, b) = (hit.highlight.start, hit.highlight.end);
                let spans: [Span<'_, (), Font>; 3] = [
                    span(&p[..a]).color(c.muted),
                    span(&p[a..b]).color(c.accent),
                    span(&p[b..]).color(c.muted),
                ];
                let preview = rich_text(spans)
                    .font(app.look.editor_font)
                    .size(app.settings.ui_font_size - 1.0)
                    .wrapping(Wrapping::None);
                let number = hint(app, (hit.line + 1).to_string())
                    .width(Length::Fixed(34.0))
                    .align_x(Alignment::End);
                list = list.push(
                    button(row![number, preview].spacing(8).align_y(Alignment::Center))
                        .width(Fill)
                        .padding(Padding { top: 2.0, bottom: 2.0, left: 4.0, right: 8.0 })
                        .style(style::row(c, false))
                        .on_press(Message::SearchOpen(f, h)),
                );
            }
        }
    }

    container(column![header, controls, scrollable(list).height(Fill).style(style::scroll(c))])
        .width(Length::Fixed(app.sidebar_width()))
        .height(Fill)
        .style(style::surface(c.panel, c.text))
        .into()
}
