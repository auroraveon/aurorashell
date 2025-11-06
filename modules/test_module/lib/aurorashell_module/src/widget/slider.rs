use std::any::Any;

use crate::{CallbackType, ElementsMemoryArena};

use super::{Element, ElementTag, RawElement, Widget};

pub struct Slider<T> {
    pub range: std::ops::RangeInclusive<T>,
    pub value: T,
    pub on_change: Option<SliderFn<T>>,
}

impl<T: SliderNumber> Slider<T> {
    pub fn new(range: std::ops::RangeInclusive<T>, value: T, on_change: SliderFn<T>) -> Self {
        Self {
            range,
            value,
            on_change: Some(on_change),
        }
    }
}

impl<'a, Message, T: SliderNumber + 'static> Widget<Message> for Slider<T> {
    fn arena_index(
        &mut self,
        arena: &mut ElementsMemoryArena,
        callbacks: &mut Vec<CallbackType>,
    ) -> u32 {
        let number_type: u8 = match T::TYPE {
            SliderNumberType::I32 => 0b00,
            SliderNumberType::F32 => 0b01,
            SliderNumberType::F64 => 0b10,
        };

        let range_min = *self.range.start();
        let range_max = *self.range.end();

        let inner = RawSliderData {
            number_type,
            range_min: range_min.to_u64_bits(),
            range_max: range_max.to_u64_bits(),
            value: self.value.clone().to_u64_bits(),
        };
        arena.slider_data.push(inner);
        let data_index = (arena.slider_data.len() - 1) as u32;

        let mut callback_index: u32 = 0;
        if let Some(callback) = self.on_change.take() {
            let callback: Box<dyn Any + Send + Sync> = Box::new(callback);
            callbacks.push(CallbackType::Slider {
                ty: T::TYPE,
                func: callback,
            });
            callback_index = callbacks.len() as u32;
        }

        let element = RawElement {
            tag: ElementTag::Slider as u8,
            child_count: 0,
            children_index: 0,
            data_index,
            callback_index,
            style_index: 0,
        };

        arena.elements.push(element);

        let index = (arena.elements.len() - 1) as u32;
        return index as u32;
    }
}

impl<'a, Message, T> From<Slider<T>> for Element<'a, Message>
where
    Message: 'a,
    T: SliderNumber + 'static,
{
    fn from(slider: Slider<T>) -> Self {
        Self::new(slider)
    }
}

pub type SliderFn<T> = Box<dyn Fn(T) -> (u32, T) + Send + Sync>;

#[repr(C)]
#[derive(Debug)]
pub struct RawSliderData {
    /// these are bitflags for what number type the slider is using
    /// 00 - `i32`
    /// 01 - `f32`
    /// 10 - `f64`
    ///
    /// `i64` not supported because the `iced::Slider` widget expects `f64` to
    /// implement the trait `From<T>`, and i64 doesn't fit that criteria
    pub number_type: u8,
    /// actual type is determined from `number_type`
    pub range_min: u64,
    /// actual type is determined from `number_type`
    pub range_max: u64,
    /// actual type is determined from `number_type`
    pub value: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SliderNumberType {
    I32,
    F32,
    F64,
}

/// used to constrain `Slider` to a few number types so it works properly on
/// the host side
pub trait SliderNumber: Clone + Copy {
    const TYPE: SliderNumberType;

    fn to_u64_bits(self) -> u64;
}
impl SliderNumber for i32 {
    const TYPE: SliderNumberType = SliderNumberType::I32;

    fn to_u64_bits(self) -> u64 {
        self as u64
    }
}
impl SliderNumber for f32 {
    const TYPE: SliderNumberType = SliderNumberType::F32;

    fn to_u64_bits(self) -> u64 {
        self.to_bits() as u64
    }
}
impl SliderNumber for f64 {
    const TYPE: SliderNumberType = SliderNumberType::F64;

    fn to_u64_bits(self) -> u64 {
        self.to_bits()
    }
}
