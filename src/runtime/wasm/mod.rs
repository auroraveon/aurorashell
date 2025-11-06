mod api;
mod de;
mod fs;
mod id;
mod messages;
mod state;
mod ui;

pub use messages::{Event, Request};
pub use state::WasmState;
pub use ui::{SliderNumberType, WasmUiNode};

use api::get_api_functions;
use fs::load_modules;
use id::WasmId;
use ui::get_element_tree;

use super::module::Register;
use super::{RuntimeEvent, RuntimeRequest, RuntimeService};

use std::any::TypeId;
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use derivative::Derivative;
use iced::Subscription;
use iced::futures::SinkExt;
use iced::futures::channel::mpsc::Sender as IcedSender;
use iced::stream::channel;
use wasmtime::{Config, Engine, Instance, Linker, Memory, Store};
use wasmtime_wasi::preview1::WasiP1Ctx;

pub trait WasmSerializable: std::fmt::Debug + Send + Sync {
    fn serialise(self) -> &'static [u8];
}

#[derive(Debug, Clone)]
pub struct WasmRuntime;

impl RuntimeService for WasmRuntime {
    type Event = Event;
    type Init = ();
    type Request = Request;
    type ServiceData = Box<dyn WasmSerializable>;
    type State = WasmState;

    fn run(_: Self::Init) -> iced::Subscription<RuntimeEvent<Self>> {
        let id = TypeId::of::<Self>();

        Subscription::run_with_id(
            id,
            channel(100, async move |mut chan| {
                loop {
                    match WasmRuntime::_run(&mut chan).await {
                        Ok(_) => {
                            log::warn!("[wasm] thread exited. restarting...");
                        }
                        Err(err) => {
                            log::error!("[wasm] crash! error: {}", err);
                        }
                    };
                }
            }),
        )
    }

    fn request(state: &mut Self::State, request: RuntimeRequest<Self>) -> anyhow::Result<()> {
        state.channel.send(request)?;
        return Ok(());
    }
}

impl WasmRuntime {
    async fn _run(chan: &mut IcedSender<RuntimeEvent<Self>>) -> anyhow::Result<()> {
        let (request_tx, request_rx) = flume::bounded::<RuntimeRequest<Self>>(100);

        let mut config = Config::new();
        config.async_support(true);
        let engine = Engine::new(&config)?;

        let mut linker: Linker<WasiContext> = Linker::new(&engine);
        wasmtime_wasi::preview1::add_to_linker_async(&mut linker, |context| &mut context.wasip1)?;

        get_api_functions(&mut linker)?;

        let mut host = WasmHost {
            engine,
            linker,
            modules: vec![],
        };

        chan.send(RuntimeEvent::Init(WasmState {
            channel: request_tx,
            surface_module_ids: HashMap::new(),
            module_ui_trees: HashMap::new(),
        }))
        .await?;

        host.modules = load_modules(&mut host, chan).await?;

        let mut modules_registers_map: Vec<(u32, Register)> = vec![];
        // assign registers from each module to a service
        for module in &host.modules {
            for register in &module.registers {
                let id = module.id;
                let register = register.clone();

                modules_registers_map.push((id, register));
            }
        }

        // need to be separated from the loop above as it says `host.modules`
        // is not `Send`
        for register in modules_registers_map {
            chan.send(RuntimeEvent::Update(Event::RegisterModuleToService {
                module_id: register.0,
                register: register.1,
            }))
            .await?;
        }

        let mut render_queue: VecDeque<u32> = VecDeque::from(
            host.modules
                .iter()
                .map(|module| module.id)
                .collect::<Vec<u32>>(),
        );

        log::debug!("[wasm] setup finished, starting loop");

        'main: loop {
            // re-render all queued modules
            'render: loop {
                let module_id = match render_queue.pop_front() {
                    Some(id) => id,
                    None => {
                        // break loop if there is no more to render
                        break 'render;
                    }
                };

                let module = &mut host.modules[module_id as usize];

                let view_func = match module
                    .instance
                    .get_typed_func::<u32, u32>(&mut module.store, "view")
                {
                    Ok(func) => func,
                    Err(err) => {
                        log::warn!(
                            "[wasm] [module:{}] view function does not exist or is incorrect \
                             type: {}",
                            module.module_name,
                            err
                        );
                        continue 'render;
                    }
                };

                let surface_ids = module.store.data().used_surface_ids.borrow().clone();
                for surface_id in surface_ids.iter() {
                    let offset = match view_func.call_async(&mut module.store, *surface_id).await {
                        Ok(res) => res,
                        Err(err) => {
                            log::warn!("[wasm] view function call failed: {err}");
                            continue;
                        }
                    };

                    let ui_tree = match get_element_tree(
                        &module.module_name,
                        &module.store,
                        module.memory,
                        offset,
                    ) {
                        Ok(tree) => tree,
                        Err(err) => {
                            log::warn!(
                                "[wasm] [module:{}] could not get tree. error: {}",
                                module.module_name,
                                err
                            );
                            continue;
                        }
                    };

                    // we must get the iced::window::Id that the surface id maps to
                    // so iced knows what surface we're actually rendering on
                    let iced_surface_id =
                        match module.store.data().surface_wasm_id.get_iced_id(&surface_id) {
                            Some(id) => id,
                            None => {
                                // this really shouldn't get ran as the surface ids are
                                // from what the module used and are checked to see
                                // if they were leased to the module
                                log::warn!(
                                    "[wasm] [module:{}] surface_id:{} somehow was not leased",
                                    module.module_name,
                                    surface_id
                                );
                                continue;
                            }
                        };

                    chan.send(RuntimeEvent::Update(Event::ModViewData {
                        module_id: module.id,
                        surface_id: *iced_surface_id,
                        tree: Box::new(ui_tree),
                    }))
                    .await?;
                }
            }

            let msg = match request_rx.recv_async().await {
                Ok(msg) => msg,
                Err(err) => {
                    log::warn!("[wasm] error while receiving message: {}", err);
                    log::warn!("[wasm] retrying in 5 seconds...");
                    thread::sleep(Duration::from_secs(5));
                    // note: shouldn't leave it like this, need to handle error
                    // at some point
                    // - aurora :3
                    continue 'main;
                }
            };

            match msg {
                RuntimeRequest::Request {
                    request:
                        Request::CallbackEvent {
                            module_id,
                            surface_id,
                            callback_id,
                            data,
                        },
                } => {
                    if let Some(module) = host.modules.get_mut(module_id as usize) {
                        // we turn the iced id to a u32 that the module knows about
                        let surface_id =
                            match module.store.data().surface_wasm_id.get_id(&surface_id) {
                                Some(id) => *id,
                                None => {
                                    log::warn!(
                                        "[wasm] [module:{}] iced surface id {} does not map to a \
                                         u32",
                                        module.module_name,
                                        surface_id
                                    );
                                    continue 'main;
                                }
                            };

                        let callback_func =
                            match module.instance.get_typed_func::<(u32, u32, u64), u64>(
                                &mut module.store,
                                "run_callback",
                            ) {
                                Ok(func) => func,
                                Err(err) => {
                                    log::warn!(
                                        "[wasm] [module:{}] run_callback function does not exist \
                                         or is incorrect type: {}",
                                        module.module_name,
                                        err
                                    );
                                    continue 'main;
                                }
                            };

                        let data_value = match data {
                            Some(data) => match data {
                                WasmCallbackData::Slider(value) => value,
                            },
                            None => 0, // no data for the associated widget
                        };

                        let callback_data = callback_func
                            .call_async(&mut module.store, (surface_id, callback_id, data_value))
                            .await?;

                        let message_id = (callback_data >> 32) as u32;
                        let data_ptr = (callback_data & u32::MAX as u64) as u32;

                        let update_func = match module
                            .instance
                            .get_typed_func::<(u32, u32), u32>(&mut module.store, "update")
                        {
                            Ok(func) => func,
                            Err(err) => {
                                eprintln!(
                                    "[wasm] [module:{}] update function does not exist or is \
                                     incorrect type: {}",
                                    module.module_name, err
                                );
                                continue 'main;
                            }
                        };
                        // note: needs to be put back into the module if its not
                        // 0 as the module might be trying to trigger side effects
                        let message_id = update_func
                            .call_async(&mut module.store, (message_id, data_ptr))
                            .await?;

                        render_queue.push_back(module_id);
                    }
                }
                _ => {}
            }
        }
    }
}

/// callback data for certain widgets
#[derive(Debug, Clone)]
pub enum WasmCallbackData {
    Slider(u64),
}

/// stores state for the wasm runtime
#[derive(Debug)]
pub struct WasmHost {
    engine: Engine,
    /// shared linker for all stores and engine
    linker: Linker<WasiContext>,
    modules: Vec<WasmModule>,
}

/// wasi context for a wasm module
#[derive(Derivative)]
#[derivative(Debug)]
struct WasiContext {
    #[derivative(Debug = "ignore")]
    pub wasip1: WasiP1Ctx,
    /// used to generate ids for surfaces
    pub surface_wasm_id: WasmId,
    /// the surface ids that the module has actually used
    pub used_surface_ids: RefCell<Vec<u32>>,
}

/// stores data related to a wasm module
#[derive(Derivative)]
#[derivative(Debug)]
struct WasmModule {
    /// the module's id :3
    id: u32,
    /// the module's name, must be unique
    module_name: String,
    /// file path in $HOME/.local/share/aurorashell/modules
    file_path: PathBuf,
    /// the registers the module has requested
    registers: Vec<Register>,
    #[derivative(Debug = "ignore")]
    /// each module gets it own store for isolation
    /// and to id it when it makes a call to us
    store: Store<WasiContext>,
    instance: Instance,
    memory: Memory,
}
