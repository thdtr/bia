//! The sidebar: the workspace's folders and files, one per row.
//!
//! Folders get a chevron and a folder icon; files get an icon for their kind
//! (see `icons.rs`). Icons are tinted with the theme's muted color, and the
//! file open in the editor is tinted with the accent.
//!
//! Creating and renaming happen in the tree itself: the name field takes
//! the place of the row, or of a new row inside the folder. A right click
//! opens a menu with everything you can do to a row; dragging a row onto a
//! folder moves it there.
//!
//! The view is rebuilt after every message, keys typed in the editor
//! included, so only the rows in sight (and a few around them) become
//! widgets. Rows all have the same height; empty space stands for the rest.

use super::menu::on_right_click;
use super::panel::icon_button;
use super::{label, style};
use crate::app::{Bia, Message, PROMPT_ID, PromptKind, TREE_ID, TreeAction};
use crate::icons;
use bia_core::{git, tree};
use iced::widget::{
    Column, column, container, mouse_area, row, scrollable, space, svg, text, text_input,
};
use iced::{Alignment, Background, Border, Element, Fill, Length, Padding, mouse};

const INDENT: f32 = 14.0;
const ICON: f32 = 16.0;
/// Rows built above and below the ones in sight, so a quick scroll doesn't
/// show empty space before the next view.
const OVERSCAN: usize = 20;

/// The height of every row: the taller of the icon and a line of text,
/// plus the padding.
pub(crate) fn row_height(app: &Bia) -> f32 {
    (app.settings.ui_font_size * 1.3).max(ICON) + 6.0
}

/// The rows to build, out of `count`, for the sidebar scrolled to
/// `offset` and `height` tall.
fn visible(count: usize, row: f32, offset: f32, height: f32) -> std::ops::Range<usize> {
    let in_sight = (height / row).ceil() as usize + 1;
    if count <= in_sight {
        return 0..count;
    }
    let first = ((offset / row).floor() as usize).min(count - in_sight);
    first.saturating_sub(OVERSCAN)..(first + in_sight + OVERSCAN).min(count)
}

pub(super) fn icon<'a>(handle: &svg::Handle, color: iced::Color) -> svg::Svg<'a> {
    svg(handle.clone())
        .width(ICON)
        .height(ICON)
        .style(move |_, _| svg::Style { color: Some(color) })
}

pub fn view(app: &Bia) -> Element<'_, Message> {
    let c = app.look.colors;
    let Some(tree) = &app.tree else { return space().into() };
    let root = tree.root();
    let target = app.target_dir_or_root();

    let header = row![
        label(app, tree.root_name()).color(c.text).wrapping(text::Wrapping::None),
        space::horizontal(),
        icon_button(
            app,
            "new-file",
            "tree.new_file",
            None,
            tree_action(TreeAction::NewFile(target.clone()))
        ),
        icon_button(
            app,
            "new-folder",
            "tree.new_folder",
            None,
            tree_action(TreeAction::NewFolder(target))
        ),
        icon_button(
            app,
            "collapse",
            "tree.collapse_all",
            None,
            tree_action(TreeAction::CollapseAll)
        ),
        icon_button(app, "sidebar", "sidebar.hide", Some("Ctrl+B"), Message::ToggleSidebar),
    ]
    .spacing(2)
    .align_y(Alignment::Center)
    .padding(Padding { top: 0.0, bottom: 0.0, left: 14.0, right: 8.0 })
    .height(Length::Fixed(38.0));

    let height = row_height(app);
    let rows = tree.rows();
    // A new file or folder gets its name field as a row of its own, first
    // inside its folder.
    let new_entry = match app.prompt.as_ref().map(|p| &p.kind) {
        Some(PromptKind::NewFile(dir) | PromptKind::NewFolder(dir)) if dir == root => Some((0, 0)),
        Some(PromptKind::NewFile(dir) | PromptKind::NewFolder(dir)) => {
            rows.iter().position(|r| &r.path == dir).map(|i| (i + 1, rows[i].depth + 1))
        }
        _ => None,
    };
    let count = rows.len() + usize::from(new_entry.is_some());
    let (offset, in_sight) = app.tree_viewport;
    let shown = visible(count, height, offset, in_sight);

    let built = shown.clone().map(|i| match new_entry {
        Some((at, depth)) if i == at => new_entry_row(app, depth, height),
        Some((at, _)) if i > at => entry_row(app, &rows[i - 1], height),
        _ => entry_row(app, &rows[i], height),
    });
    let list: Element<'_, Message> = if count == 0 {
        container(super::hint(app, app.look.i18n.t("explorer.empty"))).padding([4, 14]).into()
    } else {
        let above = space().height(height * shown.start as f32);
        let below = space().height(height * (count - shown.end) as f32);
        Column::new().push(above).extend(built).push(below).into()
    };
    let list = scrollable(mouse_area(list).on_exit(Message::TreeLeave))
        .id(TREE_ID)
        .height(Fill)
        .style(style::scroll(c))
        .on_scroll(|v| Message::TreeScrolled(v.absolute_offset().y, v.bounds().height));

    let sidebar = container(column![header, list])
        .width(Length::Fixed(app.sidebar_width()))
        .height(Fill)
        .style(style::surface(c.panel, c.text));
    // Rows only say which row a right click was on (their positions are
    // scrolled); the sidebar opens the menu where the mouse is.
    on_right_click(sidebar, Message::TreeMenu)
}

fn tree_action(action: TreeAction) -> Message {
    Message::Tree(action)
}

/// One file or folder, or its name field while it's being renamed.
fn entry_row<'a>(app: &'a Bia, entry: &'a tree::Row, height: f32) -> Element<'a, Message> {
    let c = app.look.colors;
    let open_file = app.tabs.active().map(|t| t.path.as_path());
    let is_open = open_file == Some(entry.path.as_path());
    let (chevron, kind_icon) = match (entry.is_dir, entry.expanded) {
        (true, true) => (Some(icons::get("chevron-down")), icons::get("folder-open")),
        (true, false) => (Some(icons::get("chevron-right")), icons::get("folder")),
        (false, _) => (None, icons::for_file(&entry.name)),
    };
    let chevron: Element<'_, Message> = match chevron {
        Some(handle) => icon(handle, c.muted).into(),
        None => space().width(ICON).into(),
    };
    let renaming = matches!(
        app.prompt.as_ref().map(|p| &p.kind),
        Some(PromptKind::Rename(path)) if path == &entry.path
    );
    let name: Element<'_, Message> = if renaming {
        name_field(app)
    } else {
        let name = label(app, entry.name.as_str()).wrapping(text::Wrapping::None);
        match app.git.files.get(&entry.path) {
            Some(git::FileStatus::Modified) => name.color(c.modified),
            Some(git::FileStatus::Added) => name.color(c.added),
            Some(git::FileStatus::Conflict) => name.color(c.danger),
            None => name,
        }
        .into()
    };
    let content = row![
        space().width(INDENT * entry.depth as f32),
        chevron,
        icon(kind_icon, if is_open { c.accent } else { c.muted }),
        name,
    ]
    .spacing(4)
    .align_y(Alignment::Center);

    let selected = app.selected.as_deref() == Some(entry.path.as_path());
    let hovered = app.tree_hover.as_deref() == Some(entry.path.as_path());
    let drop_here = app.drop_target().as_deref() == Some(entry.path.as_path());
    let look = container(content)
        .width(Fill)
        .height(height)
        .align_y(Alignment::Center)
        .padding(Padding { top: 3.0, bottom: 3.0, left: 8.0, right: 8.0 })
        .style(move |_| container::Style {
            background: (selected || hovered || drop_here).then_some(Background::Color(c.hover)),
            border: Border {
                color: if drop_here { c.accent } else { iced::Color::TRANSPARENT },
                width: if drop_here { 1.0 } else { 0.0 },
                radius: 0.0.into(),
            },
            ..container::Style::default()
        });
    if renaming {
        return look.into();
    }
    let path = &entry.path;
    let area = mouse_area(look)
        .on_press(Message::RowDown(path.clone()))
        .on_enter(Message::RowEnter(path.clone()))
        .interaction(if app.tree_drag.as_ref().is_some_and(|d| d.moved) {
            mouse::Interaction::Grabbing
        } else {
            mouse::Interaction::Pointer
        });
    on_right_click(area, move |_| Message::RowTarget(path.clone()))
}

/// The name field of a new file or folder, at `depth`.
fn new_entry_row(app: &Bia, depth: usize, height: f32) -> Element<'_, Message> {
    let c = app.look.colors;
    let kind = match app.prompt.as_ref().map(|p| &p.kind) {
        Some(PromptKind::NewFolder(_)) => icons::get("folder"),
        _ => icons::get("file"),
    };
    container(
        row![space().width(INDENT * depth as f32 + ICON), icon(kind, c.muted), name_field(app)]
            .spacing(4)
            .align_y(Alignment::Center),
    )
    .width(Fill)
    .height(height)
    .align_y(Alignment::Center)
    .padding(Padding { top: 1.0, bottom: 1.0, left: 8.0, right: 8.0 })
    .into()
}

/// The field where a name is typed. Enter confirms and Escape cancels; a
/// name with folders in it (`src/ui/menu.rs`) creates them too.
fn name_field(app: &Bia) -> Element<'_, Message> {
    let c = app.look.colors;
    let Some(prompt) = &app.prompt else { return space().into() };
    let failed = prompt.error.is_some();
    text_input("", &prompt.value)
        .id(PROMPT_ID)
        .on_input(Message::PromptInput)
        .on_submit(Message::PromptSubmit)
        .font(app.look.ui_font)
        .size(app.settings.ui_font_size)
        .padding([1, 4])
        .style(move |theme, status| {
            let mut style = style::input(c)(theme, status);
            if failed {
                style.border.color = c.danger;
            }
            style
        })
        .into()
}

#[cfg(test)]
mod tests {
    use super::visible;

    #[test]
    fn builds_the_rows_in_sight() {
        // Everything fits.
        assert_eq!(visible(10, 20.0, 0.0, 400.0), 0..10);
        // 3,000 rows, 21 in sight from row 1,000.
        assert_eq!(visible(3_000, 20.0, 20_000.0, 400.0), 980..1_041);
        assert_eq!(visible(3_000, 20.0, 0.0, 400.0), 0..41);
        // A scroll left over from a longer list stays at its end.
        assert_eq!(visible(100, 20.0, 50_000.0, 400.0), 59..100);
    }
}
