use iced::{Color, Radius, Theme, widget::button};

pub fn volume_button_style(theme: &Theme, _status: button::Status) -> button::Style {
    return button::Style {
        background: None,
        border_radius: Radius::new(4),
        text_color: theme.palette().primary,
        ..button::Style::default()
    };
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Base16Theme {
    background: Color,
    foreground: Color,
    color_00: Color,
    color_01: Color,
    color_02: Color,
    color_03: Color,
    color_04: Color,
    color_05: Color,
    color_06: Color,
    color_07: Color,
    color_08: Color,
    color_09: Color,
    color_10: Color,
    color_11: Color,
    color_12: Color,
    color_13: Color,
    color_14: Color,
    color_15: Color,
}

impl Base16Theme {
    pub fn from_config() {}
}
