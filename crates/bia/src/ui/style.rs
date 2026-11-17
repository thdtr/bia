//! All of BIA's colors and widget styles.
//!
//! The rules are few:
//! * two surfaces (`panel` around, `background` where you write),
//! * hairline borders instead of shadows,
//! * one accent color, used for *where you are*: the active tab marker,
//!   the open file in the explorer, the caret's selection, the hummingbird.

use bia_core::Rgba;
use iced::widget::{button, container, rule, scrollable, text_input};
use iced::{Background, Border, Color, Shadow, Theme, border};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Colors {
    pub background: Color,
    pub panel: Color,
    pub border: Color,
    pub text: Color,
    pub muted: Color,
    pub accent: Color,
    pub selection: Color,
    pub hover: Color,
    pub danger: Color,
}

fn color(c: Rgba) -> Color {
    Color::from_rgba8(c.r, c.g, c.b, c.alpha())
}

impl Colors {
    pub fn from_theme(theme: &bia_core::Theme) -> Self {
        let ui = &theme.ui;
        Self {
            background: color(ui.background),
            panel: color(ui.panel),
            border: color(ui.border),
            text: color(ui.text),
            muted: color(ui.muted),
            accent: color(ui.accent),
            selection: color(ui.selection),
            hover: color(ui.hover),
            danger: color(ui.danger),
        }
    }

    pub fn iced_theme(&self, name: &str) -> Theme {
        Theme::custom(
            name.to_owned(),
            iced::theme::Palette {
                background: self.background,
                text: self.text,
                primary: self.accent,
                success: self.accent,
                warning: self.accent,
                danger: self.danger,
            },
        )
    }
}

pub fn surface(bg: Color, fg: Color) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(bg)),
        text_color: Some(fg),
        ..container::Style::default()
    }
}

pub fn rule(c: Colors) -> impl Fn(&Theme) -> rule::Style {
    move |_| rule::Style {
        color: c.border,
        radius: border::Radius::default(),
        fill_mode: rule::FillMode::Full,
        snap: true,
    }
}

/// A flat, full-width row: explorer entries, shelf actions.
pub fn row(c: Colors, selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let bg = match status {
            _ if selected => Some(c.hover),
            button::Status::Hovered | button::Status::Pressed => Some(c.hover),
            _ => None,
        };
        button::Style {
            background: bg.map(Background::Color),
            text_color: c.text,
            border: Border::default(),
            shadow: Shadow::default(),
            snap: true,
        }
    }
}

/// Text-only button that brightens on hover.
pub fn quiet(c: Colors) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| button::Style {
        background: None,
        text_color: match status {
            button::Status::Hovered | button::Status::Pressed => c.text,
            _ => c.muted,
        },
        border: Border::default(),
        shadow: Shadow::default(),
        snap: true,
    }
}

/// A bordered button for the few explicit choices (welcome, quit dialog).
pub fn outlined(c: Colors, emphasis: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: Some(Background::Color(if hovered { c.hover } else { c.background })),
            text_color: if emphasis { c.accent } else { c.text },
            border: Border {
                color: if emphasis { c.accent } else { c.border },
                width: 1.0,
                radius: 6.0.into(),
            },
            shadow: Shadow::default(),
            snap: true,
        }
    }
}

pub fn tab(c: Colors, active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: Some(Background::Color(if active { c.background } else { c.panel })),
            text_color: if active || hovered { c.text } else { c.muted },
            border: Border::default(),
            shadow: Shadow::default(),
            snap: true,
        }
    }
}

pub fn input(c: Colors) -> impl Fn(&Theme, text_input::Status) -> text_input::Style {
    move |_, status| text_input::Style {
        background: Background::Color(c.background),
        border: Border {
            color: if matches!(status, text_input::Status::Focused { .. }) {
                c.accent
            } else {
                c.border
            },
            width: 1.0,
            radius: 4.0.into(),
        },
        icon: c.muted,
        placeholder: c.muted,
        value: c.text,
        selection: c.selection,
    }
}

pub fn scroll(c: Colors) -> impl Fn(&Theme, scrollable::Status) -> scrollable::Style {
    move |_, status| {
        let active = !matches!(status, scrollable::Status::Active { .. });
        let rail = || scrollable::Rail {
            background: None,
            border: Border::default(),
            scroller: scrollable::Scroller {
                background: Background::Color(if active { c.muted } else { c.border }),
                border: Border { radius: 3.0.into(), ..Border::default() },
            },
        };
        scrollable::Style {
            container: container::Style::default(),
            vertical_rail: rail(),
            horizontal_rail: rail(),
            gap: None,
            auto_scroll: scrollable::AutoScroll {
                background: Background::Color(c.panel),
                border: Border { color: c.border, width: 1.0, radius: 12.0.into() },
                shadow: Shadow::default(),
                icon: c.text,
            },
        }
    }
}

/// The dialog card shown over a dimmed window.
pub fn card(c: Colors) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: Some(Background::Color(c.panel)),
        text_color: Some(c.text),
        border: Border { color: c.border, width: 1.0, radius: 10.0.into() },
        ..container::Style::default()
    }
}

pub fn scrim(c: Colors) -> impl Fn(&Theme) -> container::Style {
    let mut dim = c.background;
    dim.a = 0.72;
    surface(dim, c.text)
}

/// BIA's window icon: the hummingbird from `assets/icons/bia.svg`,
/// pre-rendered to raw 64×64 RGBA so the binary needs no image decoder.
pub fn window_icon() -> Option<iced::window::Icon> {
    const RGBA: &[u8] = include_bytes!("../../../../assets/icons/bia-64.rgba");
    iced::window::icon::from_rgba(RGBA.to_vec(), 64, 64).ok()
}

/// The logo (hummingbird + "bia" in Fira Sans Light, outlined), painted with
/// the theme's text and accent colors.
pub fn logo(c: Colors) -> iced::widget::svg::Handle {
    const TEMPLATE: &str = include_str!("../../../../assets/icons/logo-template.svg");
    let hex = |c: Color| {
        let [r, g, b, _] = c.into_rgba8();
        format!("#{r:02x}{g:02x}{b:02x}")
    };
    let svg = TEMPLATE.replace("{INK}", &hex(c.text)).replace("{BIRD}", &hex(c.accent));
    iced::widget::svg::Handle::from_memory(svg.into_bytes())
}
