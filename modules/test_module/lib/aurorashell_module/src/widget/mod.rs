use crate::{CallbackType, ElementsMemoryArena};

pub(crate) mod button;
pub(crate) mod column;
pub(crate) mod row;
pub(crate) mod slider;
pub(crate) mod stack;
pub(crate) mod text;

pub use button::{Button, ButtonFn};
pub use column::Column;
pub use row::Row;
pub use slider::{Slider, SliderFn, SliderNumberType};
pub use stack::Stack;
pub use text::Text;

pub trait Widget<Message> {
    /// gets the index to the underlying RawElement that is stored
    /// in the arena so it can traverse the tree and get the ui elements
    fn arena_index(
        &mut self,
        arena: &mut ElementsMemoryArena,
        callbacks: &mut Vec<CallbackType>,
    ) -> u32;
}

pub struct Element<'a, Message> {
    pub(crate) widget: Box<dyn Widget<Message> + 'a>,
}

impl<'a, Message> Element<'a, Message> {
    #[allow(private_bounds)] // literally don't care, only the macros use it - aurora >:3
    pub fn new(widget: impl Widget<Message> + 'a) -> Self {
        Self {
            widget: Box::new(widget),
        }
    }
}

#[repr(u8)]
pub enum ElementTag {
    Row = 1,
    Column = 2,
    Text = 3,
    Button = 4,
    Slider = 5,
    Stack = 6,
}

// we use u32 to pass pointers instead of *const u8 because the host side
// could be 64 bit then it reads the pointer wrong so making both sides
// although *const u8 is 32 bits long and we can just read as u32 on the host
// side, this makes more sense for wasm as their pointers are offsets from 0
#[repr(C)]
#[derive(Debug)]
pub struct RawElement {
    pub tag: u8,
    pub child_count: u8,
    pub children_index: u32,
    pub data_index: u32,
    pub callback_index: u32,
    pub style_index: u32,
}
