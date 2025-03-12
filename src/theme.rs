use std::{collections::HashMap, env, path::PathBuf};

use config::Config;
use iced::{
    Background, Border, Color, Radius, Theme, border, color,
    core::widget::text,
    overlay::menu,
    widget::{button, pick_list, slider},
};

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Base16Theme {
    pub background: Color,
    pub foreground: Color,
    pub color00: Color,
    pub color01: Color,
    pub color02: Color,
    pub color03: Color,
    pub color04: Color,
    pub color05: Color,
    pub color06: Color,
    pub color07: Color,
    pub color08: Color,
    pub color09: Color,
    pub color10: Color,
    pub color11: Color,
    pub color12: Color,
    pub color13: Color,
    pub color14: Color,
    pub color15: Color,
}

impl Base16Theme {
    pub fn from_config() -> anyhow::Result<Self> {
        let home = match env::var("HOME") {
            Ok(v) => v,
            Err(e) => {
                eprintln!("no environment variable `HOME` or it could not be interpreted");
                return Err(e.into());
            }
        };

        let mut colors_path = PathBuf::from(home);
        colors_path.push(".config/aurorashell/colors.toml");
        let colors_path = match colors_path.to_str() {
            Some(v) => v,
            None => {
                return Err(anyhow::format_err!(
                    "could not convert {:?} to &str",
                    colors_path
                ));
            }
        };

        let colors = match Config::builder()
            .add_source(config::File::with_name(colors_path))
            .build()
        {
            Ok(v) => v,
            Err(e) => {
                eprintln!("could not get config");
                return Err(e.into());
            }
        };

        let colors = match colors.try_deserialize::<HashMap<String, String>>() {
            Ok(v) => v,
            Err(e) => {
                eprintln!("could not parse config");
                return Err(e.into());
            }
        };

        let get_key = move |key: &str| -> anyhow::Result<Color> {
            let hex_str = match colors.get(key) {
                Some(v) => v,
                None => return Err(anyhow::format_err!("could not get color: {}", key)),
            };

            if hex_str.len() != 6 {
                return Err(anyhow::format_err!(
                    "hex color does not have 6 digits: {}",
                    hex_str
                ));
            }

            let hex_color = match u32::from_str_radix(hex_str, 16) {
                Ok(v) => v,
                Err(e) => {
                    return Err(anyhow::format_err!(
                        "couldn't convert hex string to number: {}",
                        e
                    ));
                }
            };

            Ok(color!(hex_color))
        };

        return Ok(Self {
            background: get_key("background")?,
            foreground: get_key("foreground")?,
            color00: get_key("color00")?,
            color01: get_key("color01")?,
            color02: get_key("color02")?,
            color03: get_key("color03")?,
            color04: get_key("color04")?,
            color05: get_key("color05")?,
            color06: get_key("color06")?,
            color07: get_key("color07")?,
            color08: get_key("color08")?,
            color09: get_key("color09")?,
            color10: get_key("color10")?,
            color11: get_key("color11")?,
            color12: get_key("color12")?,
            color13: get_key("color13")?,
            color14: get_key("color14")?,
            color15: get_key("color15")?,
        });
    }
}

pub fn text_style(theme: &Base16Theme) -> text::StyleFn<Theme> {
    return Box::new(|_: &Theme| text::Style {
        color: Some(theme.foreground),
    });
}

pub fn pick_list_style(theme: &Base16Theme) -> pick_list::StyleFn<Theme> {
    return Box::new(|_: &Theme, _status: pick_list::Status| pick_list::Style {
        background: Background::Color(theme.color01),
        text_color: theme.foreground,
        placeholder_color: theme.color12,
        handle_color: theme.color14,
        border: border::width(1).rounded(4).color(theme.color04),
    });
}

pub fn pick_list_menu_style(theme: &Base16Theme) -> menu::StyleFn<Theme> {
    return Box::new(|_: &Theme| menu::Style {
        background: Background::Color(theme.color00),
        text_color: theme.foreground,
        selected_background: Background::Color(theme.color12),
        selected_text_color: theme.color14,
        border: border::width(1).rounded(4).color(theme.color04),
    });
}

pub fn slider_style(theme: &Base16Theme) -> slider::StyleFn<Theme> {
    return Box::new(|_: &Theme, _status: slider::Status| slider::Style {
        rail: slider::Rail {
            backgrounds: (
                Background::Color(theme.color13),
                Background::Color(theme.color00),
            ),
            width: 8.0,
            border: border::width(0).color(theme.background).rounded(128),
        },
        breakpoint: slider::Breakpoint {
            color: theme.color10,
        },
        handle: slider::Handle {
            shape: slider::HandleShape::Circle { radius: 0.0 },
            background: Background::Color(theme.color13),
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
        },
    });
}

pub fn volume_button_style(theme: &Base16Theme) -> button::StyleFn<Theme> {
    return Box::new(|_: &Theme, _status: button::Status| button::Style {
        background: None,
        text_color: theme.color05,
        border_radius: Radius::new(4),
        ..button::Style::default()
    });
}
