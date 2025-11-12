use std::cell::RefCell;
use std::path::PathBuf;
use std::{env, fs, str};

use iced::Limits as IcedLimits;
use iced::futures::channel::mpsc::Sender as IcedSender;
use iced::futures::{SinkExt, StreamExt};
use iced::platform_specific::shell::commands::layer_surface::{
    Anchor, KeyboardInteractivity, Layer,
};
use iced::runtime::platform_specific::wayland::layer_surface::{
    IcedMargin, IcedOutput, SctkLayerSurfaceSettings,
};
use wasmtime::{Module, Store};
use wasmtime_wasi::WasiCtxBuilder;

use super::de::Deserialize;
use super::id::WasmId;
use super::{Event, WasiContext, WasmHost, WasmModule, WasmRuntime};

use crate::runtime::RuntimeEvent;
use crate::services::SubscriptionData;

#[repr(C)]
#[derive(Debug)]
pub struct SetupFuncData {
    module_name_ptr: u32,
    module_name_len: u32,
    layer_surfaces_ptr: u32,
    layer_surfaces_len: u32,
    registers_bytes_ptr: u32,
}

/// represents the raw data for a `LayerSurface` so the wasm host can safely
/// read the data
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct LayerSurfaceRaw {
    pub id: u32,
    /// `Layer` gets converted to a u8
    pub layer: u8,
    /// `Anchor`'s internal value
    pub anchor: u8,
    /// 1st bit - y dir: 0 = None, 1 = Some(u32)
    /// 2nd bit - x dir: 0 = None, 1 = Some(u32)
    /// 3rd bit - size: 0 = None, 1 = Some(Option<u32>, Option<u32>)
    pub size_flags: u8,
    pub size_x: u32,
    pub size_y: u32,
    /// pointer to the Margin object
    pub margin_ptr: u32,
    /// pointer to the Limits object
    pub limits_ptr: u32,
    pub exclusive_zone: i32,
    /// `KeyboardInteractivity` gets converted to a u8
    pub keyboard_interactivity: u8,
    /// boolean for pointer interactivity is converted to a u8 to be safe
    /// to transport between wasm host and guest
    pub pointer_interactivity: u8,
}

#[repr(C)]
#[derive(Debug, Default)]
pub struct Margin {
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
    pub left: i32,
}

#[repr(C)]
#[derive(Debug)]
pub struct Limits {
    pub min_width: f32,
    pub max_width: f32,
    pub min_height: f32,
    pub max_height: f32,
}

impl LayerSurfaceRaw {
    fn into_iced(
        self,
        memory: &[u8],
        wasm_id: &WasmId,
        file_name: &str,
    ) -> Option<SctkLayerSurfaceSettings> {
        // we must get the iced::window::Id that the surface id maps to
        // so iced knows what surface we're actually rendering on
        let id = *wasm_id.get_iced_id(&self.id)?;

        let layer = match self.layer {
            0 => Layer::Background,
            1 => Layer::Bottom,
            2 => Layer::Top,
            3 => Layer::Overlay,
            _ => return None,
        };

        let anchor = Anchor::from_bits(self.anchor as u32)?;

        let mut size = None;
        // check if size was set
        if self.size_flags & 0b001 != 0 {
            size = Some((None, None));
            if let Some((ref mut x, ref mut y)) = size {
                // check if x was set
                if self.size_flags & 0b010 != 0 {
                    *x = Some(self.size_x);
                }
                // check if y was set
                if self.size_flags & 0b100 != 0 {
                    *y = Some(self.size_y);
                }
            }
        }

        let margin = {
            let offset = self.margin_ptr as usize;
            let end = offset + std::mem::size_of::<Margin>();

            if offset >= memory.len() || end >= memory.len() {
                log::error!(
                    "[wasm] [module:{}] setup_func_data: offsets out of bounds: {}-{}, memory \
                     size: {}",
                    file_name,
                    offset,
                    end,
                    memory.len()
                );
                return None;
            }

            let bytes = &memory[offset..end];

            unsafe { std::ptr::read_unaligned(bytes.as_ptr() as *const Margin) }
        };

        let margin = IcedMargin {
            top: margin.top,
            right: margin.right,
            bottom: margin.bottom,
            left: margin.left,
        };

        let limits = {
            let offset = self.limits_ptr as usize;
            let end = offset + std::mem::size_of::<Limits>();

            if offset >= memory.len() || end >= memory.len() {
                log::error!(
                    "[wasm] [module:{}] setup_func_data: offsets out of bounds: {}-{}, memory \
                     size: {}",
                    file_name,
                    offset,
                    end,
                    memory.len()
                );
                return None;
            }

            let bytes = &memory[offset..end];

            unsafe { std::ptr::read_unaligned(bytes.as_ptr() as *const Limits) }
        };

        let limits = IcedLimits::new(
            iced::Size {
                width: limits.min_width,
                height: limits.min_height,
            },
            iced::Size {
                width: limits.max_width,
                height: limits.max_height,
            },
        );

        let keyboard_interactivity = match self.keyboard_interactivity {
            0 => KeyboardInteractivity::None,
            1 => KeyboardInteractivity::Exclusive,
            2 => KeyboardInteractivity::OnDemand,
            _ => return None,
        };

        let pointer_interactivity = match self.pointer_interactivity {
            0 => false,
            1 => true,
            _ => return None,
        };

        Some(SctkLayerSurfaceSettings {
            namespace: "aurorashell".to_string(),
            output: IcedOutput::Active,
            id,
            layer,
            anchor,
            size,
            margin,
            size_limits: limits,
            exclusive_zone: self.exclusive_zone,
            keyboard_interactivity,
            pointer_interactivity,
        })
    }
}

/// gets the Instance and Memory objects for each module
/// and calls `setup()` on each to get their module name and any events
/// that they're registering to
///
/// app must have received `WasmState` before this is called
pub async fn load_modules(
    host: &mut WasmHost,
    chan: &mut IcedSender<RuntimeEvent<WasmRuntime>>,
) -> anyhow::Result<Vec<WasmModule>> {
    use std::sync::Arc;

    use tokio::sync::Mutex;

    let stream = tokio_stream::iter(get_module_paths("wasm")?);

    let host = Arc::new(Mutex::new(host));
    let chan = Arc::new(Mutex::new(chan));

    // fix: each module that fails to load needs to state that it was skipped after
    // right now that doesn't happen and just either logs an error or warning
    // stating what happened but doesn't say the module was skipped
    //
    // this can be done by first collecting all modules and their file names
    // then trying to load the rest of the module
    // if loading the module fails, say that the module with the file name,
    // was skipped :3

    let modules = Ok(stream
        .enumerate()
        .filter_map(|(id, path)| {
            let host = Arc::clone(&host);
            let chan = Arc::clone(&chan);

            async move {
                let file_name = match path.file_name() {
                    Some(res) => res,
                    None => {
                        log::error!(
                            "[wasm] [module] path does not have a file name?? path: {:?}",
                            path
                        );
                        return None;
                    }
                }
                .to_string_lossy()
                .to_string();

                let context = WasiContext {
                    wasip1: WasiCtxBuilder::new()
                        .inherit_stdout()
                        .inherit_stderr()
                        .build_p1(),
                    surface_wasm_id: Default::default(),
                    used_surface_ids: RefCell::new(vec![]),
                };

                let mut store = Store::new(&host.lock().await.engine, context);

                let module = match Module::from_file(&host.lock().await.engine, &path) {
                    Ok(res) => res,
                    Err(err) => {
                        log::error!(
                            "[wasm] [module] could not load module at `{}`, error: {}",
                            path.to_string_lossy(),
                            err
                        );
                        return None;
                    }
                };

                let instance = match host
                    .lock()
                    .await
                    .linker
                    .instantiate_async(&mut store, &module)
                    .await
                {
                    Ok(res) => res,
                    Err(err) => {
                        log::error!(
                            "[wasm] [module:{}] could not instantiate module: {}",
                            file_name,
                            err
                        );
                        return None;
                    }
                };

                let memory = match instance.get_memory(&mut store, "memory") {
                    Some(mem) => mem,
                    None => {
                        log::error!(
                            "[wasm] [module:{}] couldn't get memory from instance",
                            file_name
                        );
                        return None;
                    }
                };

                let setup_func = match instance.get_typed_func::<(), u32>(&mut store, "setup") {
                    Ok(func) => func,
                    Err(err) => {
                        log::error!(
                            "[wasm] [module:{}] setup function does not exist or is incorrect \
                             type: {}",
                            file_name,
                            err
                        );
                        return None;
                    }
                };
                let offset = match setup_func.call_async(&mut store, ()).await {
                    Ok(res) => res,
                    Err(err) => {
                        log::error!(
                            "[wasm] [module:{}] calling `setup` failed: {}",
                            file_name,
                            err
                        );
                        return None;
                    }
                };

                let memory_bytes = memory.data(&store);

                let setup_func_data = {
                    let offset = offset as usize;
                    let end = offset as usize + std::mem::size_of::<SetupFuncData>();

                    if offset >= memory_bytes.len() || end >= memory_bytes.len() {
                        log::error!(
                            "[wasm] [module:{}] setup_func_data: offsets out of bounds: \
                             {:02X}-{:02X}, memory size: {:02X}",
                            file_name,
                            offset,
                            end,
                            memory_bytes.len()
                        );
                        return None;
                    }

                    let bytes = &memory_bytes[offset..end];
                    unsafe { std::ptr::read_unaligned(bytes.as_ptr() as *const SetupFuncData) }
                };

                let module_name = {
                    let offset = setup_func_data.module_name_ptr as usize;
                    let len = setup_func_data.module_name_len as usize;
                    let end = offset + len;

                    if offset >= memory_bytes.len() || end >= memory_bytes.len() {
                        log::error!(
                            "[wasm] [module:{}] module_name: offsets out of bounds: \
                             {:02X}-{:02X}, memory size: {:02X}",
                            file_name,
                            offset,
                            end,
                            memory_bytes.len()
                        );
                        return None;
                    }

                    let bytes = &memory_bytes[offset..end];

                    match str::from_utf8(bytes).ok() {
                        Some(s) => s,
                        None => {
                            log::error!(
                                "[wasm] [module:{}] failed to get module name: failed to convert \
                                 string from bytes: {:?}",
                                file_name,
                                bytes
                            );
                            return None;
                        }
                    }
                    .to_string()
                };

                let layer_surfaces = {
                    let offset = setup_func_data.layer_surfaces_ptr as usize;
                    let len = setup_func_data.layer_surfaces_len as usize;
                    let end = offset + len * std::mem::size_of::<LayerSurfaceRaw>();

                    if offset >= memory_bytes.len() || end >= memory_bytes.len() {
                        log::error!(
                            "[wasm] [module:{}] layer_surfaces: offsets out of bounds: \
                             {:02X}-{:02X}, memory size: {:02X}",
                            file_name,
                            offset,
                            end,
                            memory_bytes.len()
                        );
                        return None;
                    }

                    let bytes = &memory_bytes[offset..end];

                    unsafe {
                        std::slice::from_raw_parts(bytes.as_ptr() as *const LayerSurfaceRaw, len)
                    }
                };

                for surface in layer_surfaces {
                    // if the id that the surface uses was leased to the module we add
                    // it to a list of ids that this module uses
                    if store.data().surface_wasm_id.has_lease(surface.id) {
                        store.data().used_surface_ids.borrow_mut().push(surface.id);
                    }
                    let layer_settings =
                        surface.into_iced(memory_bytes, &store.data().surface_wasm_id, &file_name);
                    if let Some(layer) = layer_settings {
                        // request the app to create a layer surface for us
                        match chan
                            .lock()
                            .await
                            .send(RuntimeEvent::Update(Event::CreateLayerSurface(layer)))
                            .await
                        {
                            Ok(_) => {}
                            Err(err) => {
                                log::warn!(
                                    "[wasm] [module:{}] layer surface could not be created \
                                     (skipped): {}",
                                    file_name,
                                    err
                                );
                                continue;
                            }
                        };
                    } else {
                        log::warn!(
                            "[wasm] [module:{}] layer surface invalid (skipped): {:?}",
                            file_name,
                            surface
                        );
                    }
                }

                let registers_bytes = {
                    let offset = setup_func_data.registers_bytes_ptr as usize;

                    if offset >= memory_bytes.len() || offset + 4 >= memory_bytes.len() {
                        log::error!(
                            "[wasm] [module:{}] registers: offsets out of bounds: {:02X}-{:02X}, \
                             memory size: {:02X}",
                            file_name,
                            offset,
                            offset + 4,
                            memory_bytes.len(),
                        );
                        return None;
                    }

                    let size_bytes: [u8; 4] = match memory_bytes[offset..offset + 4].try_into() {
                        Ok(bytes) => bytes,
                        Err(err) => {
                            log::error!(
                                "[wasm] [module:{}] somehow couldn't convert a slice of length 4 \
                                 to an array of length 4: {}",
                                file_name,
                                err,
                            );
                            return None;
                        }
                    };
                    let size = u32::from_be_bytes(size_bytes);

                    let end = offset + size as usize;

                    if end >= memory_bytes.len() {
                        log::error!(
                            "[wasm] [module:{}] registers: end offset out of bounds: {:02X}, \
                             memory size: {:02X}",
                            file_name,
                            end,
                            memory_bytes.len(),
                        );
                        return None;
                    }

                    let registers_bytes = &memory_bytes[offset..end];
                    registers_bytes
                };

                let registers: Vec<SubscriptionData> =
                    match Deserialize::deserialize(registers_bytes) {
                        Ok(res) => res,
                        Err(err) => {
                            log::error!(
                                "[wasm] [module:{}] could not deserialize registers bytes: {}",
                                file_name,
                                err
                            );
                            return None;
                        }
                    };

                let setup_cleanup_func =
                    match instance.get_typed_func::<(), ()>(&mut store, "setup_cleanup") {
                        Ok(func) => func,
                        Err(err) => {
                            log::error!(
                                "[wasm] [module:{}] setup_cleanup function does not exist or is \
                                 incorrect type: {}",
                                file_name,
                                err
                            );
                            return None;
                        }
                    };
                match setup_cleanup_func.call_async(&mut store, ()).await {
                    Ok(_) => {}
                    Err(err) => {
                        log::error!(
                            "[wasm] [module:{}] calling `setup_cleanup` failed: {}",
                            file_name,
                            err
                        );
                        return None;
                    }
                };

                Some(WasmModule {
                    id: id as u32,
                    module_name,
                    file_path: path,
                    registers,
                    store,
                    instance,
                    memory,
                })
            }
        })
        .collect::<Vec<WasmModule>>()
        .await);

    return modules;
}

/// get file paths for modules in $HOME/.local/share/aurorashell/modules
///
/// if the directory doesn't exist, it will be created
///
/// no filter returns files with no extension
/// "*" filter returns all files
///
/// `filter`: file extension to filter by
fn get_module_paths(filter: &str) -> anyhow::Result<Vec<PathBuf>> {
    let home_path = match env::var("HOME") {
        Ok(v) => v,
        Err(e) => {
            log::error!("[wasm] no environment variable `HOME` or it could not be interpreted");
            return Err(e.into());
        }
    };

    let path = PathBuf::from(home_path).join(".local/share/aurorashell/modules");

    if let false = path.try_exists()? {
        fs::create_dir_all(path.as_path())?;
    }

    let files = fs::read_dir(&path)?
        .filter_map(|p| match p {
            Ok(entry) => {
                if filter == "*" {
                    Some(path.join(entry.path()))
                } else {
                    match entry.path().extension() {
                        Some(ext) => {
                            if ext.to_str()? == filter {
                                Some(path.join(entry.path()))
                            } else {
                                None
                            }
                        }
                        None => {
                            if filter.len() == 0 {
                                Some(path.join(entry.path()))
                            } else {
                                None
                            }
                        }
                    }
                }
            }
            Err(_) => None,
        })
        .collect::<Vec<PathBuf>>();

    return Ok(files);
}
