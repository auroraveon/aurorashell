use crate::{CallbackType, ElementsMemoryArena};

use super::{Element, ElementTag, RawElement, Widget};

pub struct Stack<'a, Message> {
    children: Vec<Element<'a, Message>>,
}

impl<'a, Message> Stack<'a, Message> {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
        }
    }

    pub fn from_vec(children: Vec<Element<'a, Message>>) -> Self {
        Self { children }
    }
}

impl<'a, Message> Widget<Message> for Stack<'a, Message> {
    fn arena_index(
        &mut self,
        arena: &mut ElementsMemoryArena,
        callbacks: &mut Vec<CallbackType>,
    ) -> u32 {
        // get all indexes to child elements' raw element struct
        let mut children: Vec<u32> = Vec::new();
        for child in &mut self.children {
            let child_index = child.widget.arena_index(arena, callbacks);
            children.push(child_index);
        }
        let length = children.len();
        arena.children.push(children);

        let children_index = (arena.children.len() - 1) as u32;

        let element = RawElement {
            tag: ElementTag::Stack as u8,
            child_count: match u8::try_from(length).ok() {
                Some(v) => v,
                None => 255,
            },
            children_index,
            data_index: 0,
            callback_index: 0,
            style_index: 0,
        };

        arena.elements.push(element);

        let index = arena.elements.len() - 1;
        return index as u32;
    }
}

impl<'a, Message> From<Stack<'a, Message>> for Element<'a, Message>
where
    Message: 'a,
{
    fn from(row: Stack<'a, Message>) -> Self {
        Self::new(row)
    }
}

#[macro_export]
macro_rules! stack {
    () => (
        $crate::widget::Stack::new()
    );
    ($($x:expr),+ $(,)?) => (
        $crate::widget::Stack::from_vec(vec![$($crate::widget::Element::new($x)),+])
    );
}
