use crate::runtime::module::Register;
use crate::runtime::wasm::{self, WasmCallbackData, WasmRuntime, WasmState, WasmUiNode};
use crate::runtime::{RuntimeEvent, RuntimeModuleId, RuntimeRequest, RuntimeService, RuntimeState};
use crate::services::audio::AudioService;
use crate::services::{Service, ServiceEvent, ServiceRequest};
use crate::theme::Base16Color;

use iced::daemon::Appearance;
use iced::platform_specific::shell::commands::layer_surface::destroy_layer_surface;
use iced::widget::{Column, Row, Stack, button, column, container, row, slider, text};
use iced::window::Id;
use iced::{Background, Color, Element, Font, Subscription, Task, Theme, border};

#[derive(Debug)]
pub struct App {
    font: Font,
    base_16_theme: Base16Color,

    service: AppServices,
    runtime: AppRuntimes,
}

/// stores the channels required to communicate with services
///
/// disabled/unloaded services are `None` and
/// enabled/loaded are `Some(ServiceRequest<Service>)`
#[derive(Debug, Default)]
struct AppServices {
    audio: Option<flume::Sender<ServiceRequest<AudioService>>>,
}

/// stores all the state for the runtimes that the app needs to know about
/// like ui trees for modules
#[derive(Debug, Default)]
struct AppRuntimes {
    wasm: Option<WasmState>,
}

#[derive(Debug, Clone)]
pub enum AppMessage {
    /// events from a service
    Service(ServiceMessage),
    /// events from a runtime
    Runtime(RuntimeMessage),

    /// requests that need to be relayed to a service or runtime
    Request(SubscriptionRequest),
}

#[derive(Debug, Clone)]
pub enum ServiceMessage {
    Audio(ServiceEvent<AudioService>),
}

#[derive(Debug, Clone)]
pub enum RuntimeMessage {
    Wasm(RuntimeEvent<WasmRuntime>),
}

#[derive(Debug, Clone)]
pub enum SubscriptionRequest {
    Wasm(wasm::Request),
}

impl App {
    pub fn new() -> (App, Task<AppMessage>) {
        let theme = match Base16Color::from_config() {
            Ok(theme) => theme,
            Err(e) => {
                // todo: should prob not just panic but its not like i wanna
                // continue to load anyway if this fails soooooo >:3
                panic!("error occured while loading theme: {e}");
            }
        };

        (
            Self {
                font: Font::with_name("DepartureMono Nerd Font"),
                base_16_theme: theme,
                service: Default::default(),
                runtime: Default::default(),
            },
            Task::none(),
        )
    }

    pub fn title(&self, _id: Id) -> String {
        "aurorashell".to_string()
    }

    pub fn update(&mut self, message: AppMessage) -> Task<AppMessage> {
        let mut command = Task::none();

        match message {
            AppMessage::Service(event) => match event {
                ServiceMessage::Audio(event) => match event {
                    ServiceEvent::Init { request_tx } => {
                        self.service.audio = Some(request_tx);
                        log::debug!("[app] audio service initalized");
                    }
                    ServiceEvent::Update { event } => {
                        if let Some(audio) = &mut self.service.audio {
                            if let Some(wasm) = &mut self.runtime.wasm {
                                if let Err(err) = WasmRuntime::request(
                                    wasm,
                                    RuntimeRequest::ServiceData {
                                        data: Box::new(event.clone()),
                                    },
                                ) {
                                    log::error!(
                                        "[app] could not send ServiceData request to audio \
                                         service: {err}"
                                    );
                                }
                            }

                            log::trace!("[app] audio update: {event:?}");
                        } else {
                            log::error!("[app] audio service not initalized");
                        }
                    }
                },
            },
            AppMessage::Runtime(event) => match event {
                RuntimeMessage::Wasm(event) => match event {
                    RuntimeEvent::Init(init) => {
                        if let Some(wasm) = &self.runtime.wasm {
                            let mut tasks: Vec<Task<AppMessage>> = vec![];

                            // destroy all layer surfaces related to modules
                            for layer_id in wasm.surface_module_ids.keys() {
                                tasks.push(destroy_layer_surface(*layer_id));
                            }

                            command = Task::batch(tasks);
                        }

                        self.runtime.wasm = Some(init);

                        log::debug!("wasm service initalized");
                    }
                    RuntimeEvent::Update(event) => {
                        if let Some(wasm) = &mut self.runtime.wasm {
                            command = wasm.update(event.clone());

                            // note: maybe have this event separate from
                            // regular events
                            // so not part of `RuntimeEvent::Update`
                            if let wasm::Event::RegisterModuleToService {
                                module_id,
                                register,
                            } = event
                            {
                                match register {
                                    Register::Interval {
                                        milliseconds,
                                        offset,
                                    } => {}
                                    Register::PulseAudio { pulseaudio } => {}
                                }
                            }
                        } else {
                            eprintln!("[app] [wasm:update] wasm runtime not initalized");
                        }
                    }
                },
            },
            AppMessage::Request(request) => match request {
                SubscriptionRequest::Wasm(request) => {
                    if let Some(wasm) = &mut self.runtime.wasm {
                        match WasmRuntime::request(wasm, RuntimeRequest::Request { request }) {
                            Ok(_) => (),
                            Err(err) => {
                                eprintln!(
                                    "[app] [wasm] could not send request to the wasm runtime: {}",
                                    err
                                );
                            }
                        };
                    } else {
                        eprintln!("[app] [wasm:request] wasm runtime not initalized");
                    }
                }
            },
        }

        return command;
    }

    pub fn view(&self, id: Id) -> Element<'_, AppMessage> {
        if let Some(wasm) = &self.runtime.wasm {
            if let Some(module_id) = wasm.surface_module_ids.get(&id) {
                if let Some(map) = wasm.module_ui_trees.get(module_id) {
                    if let Some(tree) = map.get(&id) {
                        return build_tree(*module_id, id, &tree);
                    }
                }
            }
        }

        // note: possibly add more debug statements specifying information
        // from failed if statements above
        log::error!("could not render ui");

        // render no ui if all checks fail
        return row![].into();
    }

    pub fn subscription(&self) -> Subscription<AppMessage> {
        Subscription::batch(vec![
            Subscription::batch(vec![
                AudioService::subscribe()
                    .map(|event| AppMessage::Service(ServiceMessage::Audio(event))),
            ]),
            Subscription::batch(vec![
                WasmRuntime::run(()).map(|event| AppMessage::Runtime(RuntimeMessage::Wasm(event))),
            ]),
        ])
    }

    pub fn style(&self, theme: &Theme) -> Appearance {
        Appearance {
            background_color: Color::TRANSPARENT,
            text_color: theme.palette().text,
            icon_color: theme.palette().text,
        }
    }
}

pub fn build_tree(module_id: u32, surface_id: Id, node: &WasmUiNode) -> Element<'_, AppMessage> {
    match node {
        WasmUiNode::Row { children } => Row::with_children(
            children
                .iter()
                .map(|child| build_tree(module_id, surface_id, child))
                .collect::<Vec<Element<AppMessage>>>(),
        )
        .into(),
        WasmUiNode::Column { children } => Column::with_children(
            children
                .iter()
                .map(|child| build_tree(module_id, surface_id, child))
                .collect::<Vec<Element<AppMessage>>>(),
        )
        .into(),
        WasmUiNode::Text { content, style } => {
            let mut widget = text(content.clone()).size(11);

            widget = widget.style(Box::new(|_: &Theme| *style));

            widget.into()
        }
        WasmUiNode::Button { inner, callback_id } => {
            let mut widget = button(build_tree(module_id, surface_id, inner));

            if *callback_id != 0 {
                widget = widget.on_press_with(move || {
                    AppMessage::Request(SubscriptionRequest::Wasm(wasm::Request::CallbackEvent {
                        module_id,
                        surface_id,
                        callback_id: *callback_id,
                        data: None,
                    }))
                });
            }

            widget.into()
        }
        WasmUiNode::Slider {
            number_type,
            range,
            value,
            callback_id,
        } => match number_type {
            wasm::SliderNumberType::I32 => {
                let start = *range.start() as i32;
                let end = *range.end() as i32;
                let range = start..=end;

                slider(range, *value as i32, move |new_value| {
                    AppMessage::Request(SubscriptionRequest::Wasm(wasm::Request::CallbackEvent {
                        module_id,
                        surface_id,
                        callback_id: *callback_id,
                        data: Some(WasmCallbackData::Slider(new_value as u64)),
                    }))
                })
                .into()
            }
            wasm::SliderNumberType::F32 => {
                let start = f32::from_bits(*range.start() as u32);
                let end = f32::from_bits(*range.end() as u32);
                let range = start..=end;

                slider(range, f32::from_bits(*value as u32), move |new_value| {
                    AppMessage::Request(SubscriptionRequest::Wasm(wasm::Request::CallbackEvent {
                        module_id,
                        surface_id,
                        callback_id: *callback_id,
                        data: Some(WasmCallbackData::Slider(new_value.to_bits() as u64)),
                    }))
                })
                .into()
            }
            wasm::SliderNumberType::F64 => {
                let start = f64::from_bits(*range.start());
                let end = f64::from_bits(*range.end());
                let range = start..=end;

                slider(range, f64::from_bits(*value), move |new_value| {
                    AppMessage::Request(SubscriptionRequest::Wasm(wasm::Request::CallbackEvent {
                        module_id,
                        surface_id,
                        callback_id: *callback_id,
                        data: Some(WasmCallbackData::Slider(new_value.to_bits() as u64)),
                    }))
                })
                .into()
            }
        },
        WasmUiNode::Stack { children } => Stack::with_children(
            children
                .iter()
                .map(|child| build_tree(module_id, surface_id, child))
                .collect::<Vec<Element<AppMessage>>>(),
        )
        .into(),
    }
}
