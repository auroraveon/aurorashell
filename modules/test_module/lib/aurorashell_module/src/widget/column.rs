use crate::{CallbackType, ElementsMemoryArena};

use super::{Element, ElementTag, RawElement, Widget};

pub struct Column<'a, Message> {
    children: Vec<Element<'a, Message>>,
}

impl<'a, Message> Column<'a, Message> {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
        }
    }

    pub fn from_vec(children: Vec<Element<'a, Message>>) -> Self {
        Self { children }
    }
}

impl<'a, Message> Widget<Message> for Column<'a, Message> {
    fn arena_index(
        &mut self,
        arena: &mut ElementsMemoryArena,
        callbacks: &mut Vec<CallbackType>,
    ) -> u32 {
        // get all indexes to child elements' raw element struct
        let mut children_index = 0;
        if self.children.len() > 0 {
            let mut children: Vec<u32> = Vec::new();
            for child in &mut self.children {
                let child_index = child.widget.arena_index(arena, callbacks);
                children.push(child_index);
            }

            arena.children.push(children);

            children_index = (arena.children.len() - 1) as u32
        }

        let element = RawElement {
            tag: ElementTag::Column as u8,
            child_count: match u8::try_from(self.children.len()).ok() {
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

impl<'a, Message> From<Column<'a, Message>> for Element<'a, Message>
where
    Message: 'a,
{
    fn from(column: Column<'a, Message>) -> Self {
        Self::new(column)
    }
}

#[macro_export]
macro_rules! column {
    () => (
        $crate::widget::Column::new()
    );
    ($($x:expr),+ $(,)?) => (
        $crate::widget::Column::from_vec(vec![$($crate::widget::Element::new($x)),+])
    );
}
