use std::{
    fmt::Debug,
    sync::atomic::{AtomicPtr, Ordering},
};

use crate::{
    register::Registers,
    surface::{LayerSurface, LayerSurfaceRaw, Limits, Margin},
};

#[derive(Debug)]
pub struct SetupData {
    /// name of the module
    pub module_name: String,
    /// layer surfaces that the module can render to
    pub layer_surfaces: Vec<LayerSurface>,
    /// data and interrupts that the module can request
    pub registers: Registers,
}

impl From<SetupData> for *const SetupFuncData {
    fn from(value: SetupData) -> Self {
        // call cleanup to free already leaked memory
        // this function checks if its null first so its oki to call
        unsafe { setup_cleanup() };

        // we leak everything that needs to be sent back to the wasm host
        // so that it doesn't get freed at the end of the function so that
        // then the host can actually read it
        // the host then calls setup_cleanup() so the module can free the data

        let leaked_self = Box::leak(Box::new(value));

        let layer_surfaces_raw: Vec<LayerSurfaceRaw> = leaked_self
            .layer_surfaces
            .iter()
            .map(|surface| {
                let mut size_flags = 0u8;
                let mut size_x = 0u32;
                let mut size_y = 0u32;

                if let Some((x, y)) = surface.size {
                    // 1st bit
                    size_flags = size_flags | 0b001;
                    if let Some(x) = x {
                        // 2nd bit
                        size_flags = size_flags | 0b010;
                        size_x = x;
                    }
                    if let Some(y) = y {
                        // 3rd bit
                        size_flags = size_flags | 0b100;
                        size_y = y;
                    }
                }

                LayerSurfaceRaw {
                    id: surface.id.get_id(),
                    layer: surface.layer.clone() as u8,
                    anchor: surface.anchor.0,
                    size_flags,
                    size_x,
                    size_y,
                    margin_ptr: (&surface.margin as *const Margin) as u32,
                    limits_ptr: (&surface.limits as *const Limits) as u32,
                    exclusive_zone: surface.exclusive_zone,
                    keyboard_interactivity: surface.keyboard_interactivity.clone() as u8,
                    pointer_interactivity: match surface.pointer_interactivity {
                        false => 0,
                        true => 1,
                    },
                }
            })
            .collect();

        let leaked_layer_surfaces = Box::leak(Box::new(layer_surfaces_raw));

        let leaked_register_bytes = Box::leak(leaked_self.registers.serialize());
        println!("{:?}", leaked_register_bytes);

        let leaked_data = Box::leak(Box::new(SetupFuncData {
            module_name_ptr: leaked_self.module_name.as_ptr() as u32,
            module_name_len: leaked_self.module_name.len() as u32,
            layer_surfaces_ptr: leaked_layer_surfaces.as_ptr() as u32,
            layer_surfaces_len: leaked_layer_surfaces.len() as u32,
            registers_bytes_ptr: leaked_register_bytes.as_ptr() as u32,
        }));

        let leaked_cleanup = Box::leak(Box::new(SetupCleanup {
            data_ptr: leaked_self as *mut SetupData,
            func_ptr: leaked_data as *mut SetupFuncData,
            layer_surfaces_ptr: leaked_layer_surfaces as *mut Vec<LayerSurfaceRaw>,
            registers_bytes_ptr: leaked_register_bytes as *mut [u8],
        }));
        // store the pointers in global memory
        SETUP_CLEANUP_PTR.store(leaked_cleanup as *mut SetupCleanup, Ordering::Relaxed);

        return leaked_data as *const SetupFuncData;
    }
}

#[repr(C)]
#[derive(Debug)]
pub struct SetupFuncData {
    module_name_ptr: u32,
    module_name_len: u32,
    layer_surfaces_ptr: u32,
    layer_surfaces_len: u32,
    registers_bytes_ptr: u32,
}

/// stores pointers so that when setup_cleanup() is called, we know where the
/// data is to clean it up
#[derive(Debug)]
struct SetupCleanup {
    data_ptr: *mut SetupData,
    func_ptr: *mut SetupFuncData,
    layer_surfaces_ptr: *mut Vec<LayerSurfaceRaw>,
    registers_bytes_ptr: *mut [u8],
}

// note: couldn't make this as a type of *mut SetupCleanup so i just made it
// an atomic usize instead which is the same size as a pointer
// still not safe multi-threaded but i don't care because
// i don't run the module multi-threaded - an Arc would fix this
// ordering will need to be changed from Ordering::Relaxed as that is not
// thread safe
//     - aurora :3
static SETUP_CLEANUP_PTR: AtomicPtr<SetupCleanup> = AtomicPtr::new(std::ptr::null_mut());

/// cleans up after the `setup` function
///
/// note: `setup` function is implemented separate from this library
#[unsafe(no_mangle)]
unsafe fn setup_cleanup() {
    if SETUP_CLEANUP_PTR.load(Ordering::Relaxed).is_null() {
        return;
    }

    unsafe {
        // Box::from_raw() takes ownership and calls drop automatically for us
        let cleanup_data =
            Box::from_raw(SETUP_CLEANUP_PTR.swap(std::ptr::null_mut(), Ordering::Relaxed));

        let data = Box::from_raw(cleanup_data.data_ptr);
        drop(data);

        let func = Box::from_raw(cleanup_data.func_ptr);
        drop(func);

        let layer_surfaces = Box::from_raw(cleanup_data.layer_surfaces_ptr);
        drop(layer_surfaces);

        let registers_bytes = Box::from_raw(cleanup_data.registers_bytes_ptr);
        drop(registers_bytes);
    };
}
