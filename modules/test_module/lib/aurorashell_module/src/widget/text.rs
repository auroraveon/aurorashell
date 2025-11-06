use std::borrow::Cow;

use crate::{CallbackType, ElementsMemoryArena, theme::Color};

use super::{Element, ElementTag, RawElement, Widget};

pub struct Text<'a> {
    pub fragment: Fragment<'a>,
    pub style: Option<Style>,
}

impl<'a> Text<'a> {
    pub fn new(fragment: impl IntoFragment<'a>) -> Self {
        Self {
            fragment: fragment.into_fragment(),
            style: None,
        }
    }

    pub fn style(mut self, style: Style) -> Self {
        self.style = Some(style);
        self
    }
}

impl<'a, Message> Widget<Message> for Text<'a> {
    fn arena_index(&mut self, arena: &mut ElementsMemoryArena, _: &mut Vec<CallbackType>) -> u32 {
        arena.text_strings.push(self.fragment.to_string());
        let raw_data = RawTextData {
            content_ptr: arena.text_strings[arena.text_strings.len() - 1].as_ptr() as u32,
            content_len: self.fragment.len() as u32,
        };

        arena.text_data.push(raw_data);
        let data_index = (arena.text_data.len() - 1) as u32;

        let mut style_index = 0;
        if let Some(style) = &self.style {
            let raw_style = RawStyle {
                text_color: match &style.text_color {
                    Some(color) => color.into(),
                    None => 0,
                },
            };
            arena.text_style.push(raw_style);
            style_index = arena.text_style.len() as u32;
        }

        let element = RawElement {
            tag: ElementTag::Text as u8,
            child_count: 0,
            children_index: 0,
            data_index,
            callback_index: 0,
            style_index,
        };

        arena.elements.push(element);

        let index = (arena.elements.len() - 1) as u32;
        return index as u32;
    }
}

#[repr(C)]
#[derive(Debug)]
pub struct RawTextData {
    pub content_ptr: u32,
    pub content_len: u32,
}

impl<'a, Message> From<Text<'a>> for Element<'a, Message> {
    fn from(text: Text<'a>) -> Self {
        Self::new(text)
    }
}

pub type Fragment<'a> = Cow<'a, str>;

pub trait IntoFragment<'a> {
    fn into_fragment(self) -> Fragment<'a>;
}

impl<'a> IntoFragment<'a> for Fragment<'a> {
    fn into_fragment(self) -> Fragment<'a> {
        self
    }
}

impl<'a, 'b> IntoFragment<'a> for &'a Fragment<'b> {
    fn into_fragment(self) -> Fragment<'a> {
        Fragment::Borrowed(self)
    }
}

impl<'a> IntoFragment<'a> for &'a str {
    fn into_fragment(self) -> Fragment<'a> {
        Fragment::Borrowed(self)
    }
}

impl<'a> IntoFragment<'a> for &'a String {
    fn into_fragment(self) -> Fragment<'a> {
        Fragment::Borrowed(self.as_str())
    }
}

impl<'a> IntoFragment<'a> for String {
    fn into_fragment(self) -> Fragment<'a> {
        Fragment::Owned(self)
    }
}

/// style of the `Text` widget
#[derive(Debug)]
pub struct Style {
    /// color of the text
    text_color: Option<Color>,
}

/// style of the `Text` widget
#[derive(Debug)]
pub struct RawStyle {
    /// color of the text
    text_color: u8,
}
