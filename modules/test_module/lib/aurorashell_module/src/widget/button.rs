use crate::{CallbackType, ElementsMemoryArena};

use super::{Element, ElementTag, RawElement, Widget};

pub type ButtonFn = Box<dyn Send + Sync + Fn() -> u32>;

pub struct Button<'a, Message> {
    pub inner: Element<'a, Message>,
    pub callback: Option<ButtonFn>,
}

impl<'a, Message> Button<'a, Message> {
    pub fn new(inner: Element<'a, Message>) -> Self {
        Self {
            inner,
            callback: None,
        }
    }

    pub fn on_press(mut self, f: ButtonFn) -> Self {
        self.callback = Some(f);
        self
    }
}

impl<'a, Message> Widget<Message> for Button<'a, Message> {
    fn arena_index(
        &mut self,
        arena: &mut ElementsMemoryArena,
        callbacks: &mut Vec<CallbackType>,
    ) -> u32 {
        let inner = vec![self.inner.widget.arena_index(arena, callbacks)];
        arena.children.push(inner);
        let children_index = (arena.children.len() - 1) as u32;

        let mut callback_index: u32 = 0;
        if let Some(callback) = self.callback.take() {
            callbacks.push(CallbackType::Button(callback));
            callback_index = callbacks.len() as u32;
        }

        let element = RawElement {
            tag: ElementTag::Button as u8,
            child_count: 1,
            children_index,
            data_index: 0,
            callback_index,
            style_index: 0,
        };

        arena.elements.push(element);

        let index = (arena.elements.len() - 1) as u32;
        return index as u32;
    }
}

impl<'a, Message> From<Button<'a, Message>> for Element<'a, Message>
where
    Message: 'a,
{
    fn from(button: Button<'a, Message>) -> Self {
        Self::new(button)
    }
}
