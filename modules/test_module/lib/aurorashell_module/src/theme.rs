#[repr(u8)]
#[derive(Debug, Clone)]
pub enum Color {
    Color01 = 01,
    Color02 = 02,
    Color03 = 03,
    Color04 = 04,
    Color05 = 05,
    Color06 = 06,
    Color07 = 07,
    Color08 = 08,
    Color09 = 09,
    Color10 = 10,
    Color11 = 11,
    Color12 = 12,
    Color13 = 13,
    Color14 = 14,
    Color15 = 15,
    Color16 = 16,
    ColorForeground = 17,
    ColorBackground = 18,
    Custom(Box<String>) = 19,
}

impl From<&Color> for u8 {
    fn from(color: &Color) -> Self {
        match color {
            Color::Color01 => 01,
            Color::Color02 => 02,
            Color::Color03 => 03,
            Color::Color04 => 04,
            Color::Color05 => 05,
            Color::Color06 => 06,
            Color::Color07 => 07,
            Color::Color08 => 08,
            Color::Color09 => 09,
            Color::Color10 => 10,
            Color::Color11 => 11,
            Color::Color12 => 12,
            Color::Color13 => 13,
            Color::Color14 => 14,
            Color::Color15 => 15,
            Color::Color16 => 16,
            Color::ColorForeground => 17,
            Color::ColorBackground => 18,
            Color::Custom(_) => 19,
        }
    }
}
