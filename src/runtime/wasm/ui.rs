use std::ops::RangeInclusive;
use std::str;

use anyhow::anyhow;
use iced::Color;
use iced::core::widget::text;
use wasmtime::{Memory, Store};

use super::WasiContext;

/// gets the tree of RawElement from the guest,
/// turning it into a tree of UiNode to send to the main thread
///
/// `store` - the wasmtime `Store` struct, in this case, &Store<WasiP1Ctx>
///           as we use the wasi p1
/// `memory` - the wasmtime `Memory` struct
/// `offset` - points to head of the tree in wasm linear memory
pub fn get_element_tree(
    module_name: &str,
    store: &Store<WasiContext>,
    memory: Memory,
    offset: u32,
) -> anyhow::Result<WasmUiNode> {
    let memory_bytes: &[u8] = memory.data(store);

    let data = unsafe {
        let offset = offset as usize;
        let bytes = &memory_bytes[offset..offset + std::mem::size_of::<ViewFuncData>()];
        std::ptr::read_unaligned(bytes.as_ptr() as *const ViewFuncData)
    };

    let head_element = get_raw_element(memory_bytes, &data, data.head_index)?;

    return build_tree(module_name, memory_bytes, &data, &head_element);
}

fn build_tree(
    module_name: &str,
    memory: &[u8],
    data: &ViewFuncData,
    element: &RawElement,
) -> anyhow::Result<WasmUiNode> {
    let element = match element.tag {
        1 => {
            let children = get_element_children(memory, &data, &element)?
                .iter()
                .map(|child| build_tree(module_name, memory, data, child))
                .collect::<anyhow::Result<Vec<WasmUiNode>>>()?;

            WasmUiNode::Row { children }
        }
        2 => {
            let children = get_element_children(memory, &data, &element)?
                .iter()
                .map(|child| build_tree(module_name, memory, data, child))
                .collect::<anyhow::Result<Vec<WasmUiNode>>>()?;

            WasmUiNode::Column { children }
        }
        3 => {
            let text_content = {
                // indexes to the RawTextData struct
                // assuming its an array, using `element.data_index` to offset
                // the ptr to the element
                let offset = data.raw_text_data_ptr as usize
                    + std::mem::size_of::<RawTextData>() * element.data_index as usize;
                let end = offset + std::mem::size_of::<RawTextData>();

                // note: consider turning this into a function where it returns
                // the bytes if successful, otherwise, it doesn't
                if offset >= memory.len() || end >= memory.len() {
                    return Err(anyhow::anyhow!(
                        "[wasm] [module:{}] RawTextData offsets out of bounds: {}-{}, memory \
                         size: {}",
                        module_name,
                        offset,
                        end,
                        memory.len()
                    ));
                }

                let bytes = &memory[offset..end];

                let raw_text_data: RawTextData =
                    unsafe { std::ptr::read_unaligned(bytes.as_ptr() as *const RawTextData) };

                let offset = raw_text_data.content_ptr as usize;
                let len = raw_text_data.content_len as usize;

                let bytes = &memory[offset..offset + len];

                match str::from_utf8(bytes).ok() {
                    Some(s) => s,
                    None => {
                        return Err(anyhow!(
                            "failed to convert string from bytes:\nbytes = {:?}\nlossy string = \
                             {:?}",
                            bytes,
                            String::from_utf8_lossy(bytes)
                        ));
                    }
                }
                .to_string()
            };

            let raw_style: RawTextStyle = {
                // indexes to the RawTextData struct
                // assuming its an array, using `element.data_index` to offset
                // the ptr to the element
                let offset = data.text_style_ptr as usize + std::mem::size_of::<RawTextStyle>();
                let end = offset + std::mem::size_of::<RawTextStyle>();

                if offset >= memory.len() || end >= memory.len() {
                    return Err(anyhow::anyhow!(
                        "[wasm] [module:{}] RawTextData offsets out of bounds: {}-{}, memory \
                         size: {}",
                        module_name,
                        offset,
                        end,
                        memory.len()
                    ));
                }

                let bytes = &[offset..end];

                unsafe { std::ptr::read_unaligned(bytes.as_ptr() as *const RawTextStyle) }
            };

            let style = text::Style {
                color: Some(Color::from_rgb(1.0, 1.0, 1.0)),
            };

            WasmUiNode::Text {
                content: text_content,
                style,
            }
        }
        4 => {
            let inner_element = get_element_children(memory, &data, &element)?
                .iter()
                .map(|child| build_tree(module_name, memory, data, child))
                .collect::<anyhow::Result<Vec<WasmUiNode>>>()?[0]
                .clone();

            WasmUiNode::Button {
                inner: Box::new(inner_element),
                callback_id: element.callback_id,
            }
        }
        5 => {
            let slider_data = {
                // indexes into the start of a RawSliderData element
                let offset = data.raw_slider_data_ptr as usize
                    + std::mem::size_of::<RawSliderData>() * element.data_index as usize;
                let end = offset + std::mem::size_of::<RawSliderData>();

                let bytes = &memory[offset..end];

                unsafe { std::ptr::read_unaligned(bytes.as_ptr() as *const RawSliderData) }
            };

            let number_type = match slider_data.number_type {
                0 => SliderNumberType::I32,
                1 => SliderNumberType::F32,
                2 => SliderNumberType::F64,
                n => {
                    return Err(anyhow!(
                        "[wasm] [module:{}] slider number type unsupported: {}",
                        module_name,
                        n
                    ));
                }
            };

            WasmUiNode::Slider {
                number_type,
                range: slider_data.range_min..=slider_data.range_max,
                value: slider_data.value,
                callback_id: element.callback_id,
            }
        }
        6 => {
            let children = get_element_children(memory, &data, &element)?
                .iter()
                .map(|child| build_tree(module_name, memory, data, child))
                .collect::<anyhow::Result<Vec<WasmUiNode>>>()?;

            WasmUiNode::Stack { children }
        }
        id => {
            return Err(anyhow!(
                "[wasm] [module:{}] tag unsupported: {}",
                module_name,
                id
            ));
        }
    };

    return Ok(element);
}

/// gets the raw element from the wasm module's memory
///
/// will error if the offset provides ends up out of bounds
fn get_raw_element(memory: &[u8], data: &ViewFuncData, index: u32) -> anyhow::Result<RawElement> {
    let offset = data.elements_ptr as usize + std::mem::size_of::<RawElement>() * index as usize;
    let end = offset + std::mem::size_of::<RawElement>();

    if offset >= memory.len() || end >= memory.len() {
        return Err(anyhow::anyhow!(
            "[wasm] get_raw_element: offsets out of bounds: {}-{}, memory size: {}",
            offset,
            end,
            memory.len()
        ));
    }

    let bytes = &memory[offset..end];

    let element: RawElement =
        unsafe { std::ptr::read_unaligned(bytes.as_ptr() as *const RawElement) };

    return Ok(element);
}

/// gets an element's children from the wasm module's memory
///
/// will error if the offset provides ends up out of bounds
fn get_element_children(
    memory: &[u8],
    data: &ViewFuncData,
    element: &RawElement,
) -> anyhow::Result<Vec<RawElement>> {
    if element.child_count == 0 {
        return Ok(Vec::new());
    }

    let indexes = {
        // i think i'll forget all of this so:
        // this part gets the 4 bytes that make up the offset in wasm memory
        // to the actual children vector of the element that we want
        let offset = (data.children_ptr
            + std::mem::size_of::<u32>() as u32 * element.children_index)
            as usize;
        let end = offset + std::mem::size_of::<u32>();

        if offset >= memory.len() || end >= memory.len() {
            return Err(anyhow::anyhow!(
                "[wasm] get_element_children: offsets out of bounds: {}-{}, memory size: {}",
                offset,
                end,
                memory.len()
            ));
        }

        let bytes = &memory[offset..end];

        // and this offset is the offset in wasm memory to that children vector
        let offset =
            u32::from_le_bytes(bytes.try_into().expect("no clue how its not 4 bytes :3")) as usize;
        let len = element.child_count as usize;
        // need to use u32 as usize can be 64 bit on the wasm host
        let end = offset + (std::mem::size_of::<u32>() * len);

        if offset >= memory.len() || end >= memory.len() {
            return Err(anyhow::anyhow!(
                "[wasm] get_element_children: offsets out of bounds: {}-{}, memory size: {}",
                offset,
                end,
                memory.len()
            ));
        }
        let bytes = &memory[offset..end];

        bytes
            .chunks(4)
            .map(|bytes| {
                u32::from_le_bytes(
                    bytes
                        .try_into()
                        .expect("it was supposed to be exactly 4 bytes :p"),
                )
            })
            .collect::<Vec<u32>>()
    };

    return indexes
        .iter()
        .map(|&index| get_raw_element(memory, data, index))
        .collect();
}

#[derive(Debug, Clone)]
pub enum WasmUiNode {
    Row {
        children: Vec<WasmUiNode>,
    },
    Column {
        children: Vec<WasmUiNode>,
    },
    Text {
        content: String,
        style: text::Style,
    },
    Button {
        inner: Box<WasmUiNode>,
        callback_id: u32,
    },
    Slider {
        number_type: SliderNumberType,
        range: RangeInclusive<u64>,
        value: u64,
        callback_id: u32,
    },
    Stack {
        children: Vec<WasmUiNode>,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum SliderNumberType {
    I32,
    F32,
    F64,
}

/// data that a module's `view()` function is expected to return
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct ViewFuncData {
    pub head_index: u32,
    pub elements_ptr: u32,
    pub children_ptr: u32,
    pub raw_text_data_ptr: u32,
    pub text_style_ptr: u32,
    pub raw_slider_data_ptr: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct RawElement {
    /// determines the type of element and influences the meaning of the
    /// other fields of the RawElement
    pub tag: u8,
    /// number of children the element has
    ///
    /// is greater than 0 on elements that can have children
    ///
    /// if its greater than 0 on elements that aren't, thats a bug
    pub child_count: u8,
    /// the index into the memory arena of the module
    ///
    /// 0 is a valid index and doesn't mean none
    /// this index is ignored if the element:
    /// - cannot have children
    /// - can have children, but child_count is 0
    pub children_index: u32,
    /// the index into the memory arena of the module
    ///
    /// 0 is a valid index and doesn't mean none
    /// this index is ignored if the element cannot have data,
    /// otherwise, it must
    pub data_index: u32,
    /// the id for the callback within a module
    ///
    /// 0 means no callback
    pub callback_id: u32,
    /// the index into the memory arena of the module
    ///
    /// the array that this indexes into is determined by the widget type
    ///
    /// 0 is a valid index and doesn't mean none
    /// if the element can have a style, this will have meaning
    pub style_index: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct RawTextData {
    pub content_ptr: u32,
    pub content_len: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct RawTextStyle {
    pub text_color: u8,
}

#[repr(C)]
#[derive(Debug)]
struct RawSliderData {
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
