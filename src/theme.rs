use iced::{Radius, Theme, widget::button};

use config::Config;

pub fn volume_button_style(theme: &Theme, _status: button::Status) -> button::Style {
    return button::Style {
        background: None,
        border_radius: Radius::new(4),
        text_color: theme.palette().primary,
        ..button::Style::default()
    };
}

//#[derive(Debug, Clone, Copy, Default)]
//pub struct Base16Theme;

//impl From<Base16Theme> for Theme {
//fn from(_: Base16Theme) -> Self {
//Theme::Custom(Box::new(Base16Theme))
//}
//}
