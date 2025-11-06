use std::{
    any::Any,
    collections::HashMap,
    fmt::Debug,
    sync::{LazyLock, Mutex},
};

use crate::widget::{
    ButtonFn, Element, RawElement, SliderFn, SliderNumberType,
    slider::RawSliderData,
    text::{self, RawTextData},
};

/// used as part of the exposed `view()` function to store element data
/// for the host to read
#[repr(C)]
#[derive(Debug)]
pub struct ElementsMemoryArena {
    pub(crate) elements: Vec<RawElement>,
    pub(crate) children: Vec<Vec<u32>>,
    /// pointers to the vectors in the `children` field
    //
    // dev note: not sure if this can be improved as i don't think its stable
    // to rely on the pointer staying in the same position in a `Vec`
    // so this is the best i could do - aurora :3
    pub(crate) children_ptrs: Vec<u32>,

    pub(crate) text_strings: Vec<String>,
    pub(crate) text_data: Vec<RawTextData>,
    pub(crate) text_style: Vec<text::RawStyle>,

    pub(crate) slider_data: Vec<RawSliderData>,
}

impl ElementsMemoryArena {
    pub fn new() -> Self {
        Self {
            elements: vec![],
            children: vec![],
            children_ptrs: vec![],
            text_strings: vec![],
            text_data: vec![],
            text_style: vec![],
            slider_data: vec![],
        }
    }
}

#[repr(C)]
#[derive(Debug)]
/// struct used for telling the host where offsets into the arena are
pub struct ViewFuncData {
    /// the index to the head of the tree
    pub(crate) head_index: u32,
    /// pointer to `ElementsMemoryArena.elements`
    pub(crate) elements_ptr: u32,
    /// pointer to `ElementsMemoryArena.children_ptrs`
    pub(crate) children_ptr: u32,
    /// pointer to `ElementsMemoryArena.raw_text_data`
    pub(crate) text_data_ptr: u32,
    /// pointer to `ElementsMemoryArena.text_style`
    pub text_style_ptr: u32,
    /// pointer to `ElementsMemoryArena.raw_slider_data`
    pub(crate) slider_data_ptr: u32,
}

impl ViewFuncData {
    pub fn new() -> Self {
        Self {
            head_index: 0,
            elements_ptr: 0,
            children_ptr: 0,
            text_data_ptr: 0,
            text_style_ptr: 0,
            slider_data_ptr: 0,
        }
    }
}

pub enum CallbackType {
    Button(ButtonFn),
    Slider {
        ty: SliderNumberType,
        func: Box<dyn Any + Send + Sync>,
    },
}

static ARENA: LazyLock<Mutex<ElementsMemoryArena>> =
    LazyLock::new(|| Mutex::new(ElementsMemoryArena::new()));
static VIEW_FUNC_DATA: LazyLock<Mutex<ViewFuncData>> =
    LazyLock::new(|| Mutex::new(ViewFuncData::new()));
static CALLBACKS_MAP: LazyLock<Mutex<HashMap<u32, Vec<CallbackType>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn view_build_ui<Message>(mut root_element: Element<Message>, id: u32) -> *const ViewFuncData
where
    Message: Send + Sync + Debug + 'static,
{
    let mut arena = ARENA.lock().unwrap();
    *arena = ElementsMemoryArena::new();

    let mut callbacks_map = CALLBACKS_MAP.lock().unwrap();

    let mut callbacks = match callbacks_map.get_mut(&id) {
        Some(res) => {
            *res = vec![];
            res
        }
        None => {
            callbacks_map.insert(id, vec![]);
            callbacks_map.get_mut(&id).unwrap()
        }
    };

    let index = root_element.widget.arena_index(&mut arena, &mut callbacks);

    arena.children_ptrs = arena.children.iter().map(|v| v.as_ptr() as u32).collect();

    let mut view_func_data = VIEW_FUNC_DATA.lock().unwrap();
    *view_func_data = ViewFuncData {
        head_index: index,
        elements_ptr: arena.elements.as_ptr() as u32,
        children_ptr: arena.children_ptrs.as_ptr() as u32,
        text_data_ptr: arena.text_data.as_ptr() as u32,
        text_style_ptr: arena.text_style.as_ptr() as u32,
        slider_data_ptr: arena.slider_data.as_ptr() as u32,
    };

    return &*view_func_data as *const ViewFuncData;
}

/// defines an external function to be called by the wasm host
/// to run a callback via its id and optionally a pointer
///
/// `data` can either be data or a ptr to data depending on the type of callback
#[unsafe(no_mangle)]
fn run_callback(surface_id: u32, callback_id: u32, data: u64) -> u64 {
    // id of 0 means no callback
    if callback_id == 0 {
        return 0;
    }

    let callbacks = CALLBACKS_MAP.lock().unwrap();

    let callback = match callbacks.get(&surface_id) {
        Some(callbacks) => {
            // index cannot be 0 as that would end up being -1 here
            // the function guards against this though :3
            let index = callback_id as usize - 1;
            match callbacks.get(index) {
                Some(callback) => callback,
                None => {
                    eprintln!(
                        "module: surface {} has no callback with id {}",
                        surface_id, callback_id
                    );
                    return 0;
                }
            }
        }
        None => {
            eprintln!("module: surface {} has no callbacks", surface_id);
            return 0;
        }
    };

    // (message_id, data_ptr)
    let data: (u32, u32) = match callback {
        CallbackType::Button(func) => (func(), 0),
        CallbackType::Slider { ty, func } => match ty {
            SliderNumberType::I32 => {
                if let Some(func) = func.downcast_ref::<SliderFn<i32>>() {
                    let input: i32 = data as i32;
                    let (message_id, data) = func(input);

                    let leaked_data = Box::leak(Box::new(data));
                    let data_ptr = leaked_data as *mut i32;

                    (message_id, data_ptr as u32)
                } else {
                    (0, 0)
                }
            }
            SliderNumberType::F32 => {
                if let Some(func) = func.downcast_ref::<SliderFn<f32>>() {
                    let input: f32 = f32::from_bits(data as u32);
                    let (message_id, data) = func(input);

                    let leaked_data = Box::leak(Box::new(data));
                    let data_ptr = leaked_data as *mut f32;

                    (message_id, data_ptr as u32)
                } else {
                    (0, 0)
                }
            }
            SliderNumberType::F64 => {
                if let Some(func) = func.downcast_ref::<SliderFn<f64>>() {
                    let input: f64 = f64::from_bits(data);
                    let (message_id, data) = func(input);

                    let leaked_data = Box::leak(Box::new(data));
                    let data_ptr = leaked_data as *mut f64;

                    (message_id, data_ptr as u32)
                } else {
                    (0, 0)
                }
            }
        },
    };

    // merge message id and data ptr into one u64
    let return_data = (data.0 as u64) << 32 | data.1 as u64;
    return_data
}
