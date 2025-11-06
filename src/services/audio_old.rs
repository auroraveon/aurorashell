use crate::runtime::RuntimeModuleId;
use crate::runtime::module::AudioRegisterData;
use crate::runtime::wasm::WasmSerializable;

use super::{ActiveService, PassiveService, ServiceChannel, ServiceEvent, ServiceRequest};

use std::any::TypeId;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use std::time::{Duration, Instant};

use flume::{Receiver, Sender};
use iced::futures::SinkExt;
use iced::futures::channel::mpsc::Sender as IcedSender;
use iced::{Subscription, stream::channel};
use pulse::callbacks::ListResult;
use pulse::context::introspect::Introspector;
use pulse::context::subscribe::{self, InterestMaskSet};
use pulse::context::{Context, FlagSet};
use pulse::mainloop::standard::{IterateResult, Mainloop};
use pulse::proplist::{Proplist, properties};
use pulse::volume::{ChannelVolumes, Volume};

/// the time between requesting pulseaudio to update a value that can
/// be frequently updated (like volume) to stop us from spamming the server
/// which is visually laggy
const UPDATE_INTERVAL: Duration = Duration::from_millis(100);

/// 65536 represents 100% in pulseaudio
pub const PULSE_MAX_VOLUME: u32 = 65536;

/// messages emitted from the pulseaudio client when an event happens
#[derive(Debug, Clone)]
pub enum Event {
    /// event emitted when any property of any sink (output) changes
    ///
    /// emitted as a main event from the pulseaudio mainloop
    SinksChanged { sinks: Vec<Sink> },
    /// name of the default sink
    ///
    /// event emitted when properties of the default sink (output) change
    ///
    /// emitted as a main event from the pulseaudio mainloop
    DefaultSinkChanged { name: Option<String> },

    /// event emitted when any property of any source (input) changes
    ///
    /// emitted as a main event from the pulseaudio mainloop
    SourcesChanged { sources: Vec<Source> },
    /// name of the default source
    ///
    /// event emitted when properties of the default source (input) change
    ///
    /// emitted as a main event from the pulseaudio mainloop
    DefaultSourceChanged { name: Option<String> },

    /// event emitted when any property of any card changes
    ///
    /// emitted as a main event from the pulseaudio mainloop
    CardsChanged { cards: Vec<Card> },

    /// event emitted when any associated card or default sink has changed
    ///
    /// emitted as a secondary event as a side effect of processing a main
    /// event from the pulseaudio mainloop (see `AudioState::update()`)
    SinkProfileChanged { profile_name: Option<String> },
    /// event emitted when any associated card or default source has changed
    ///
    /// emitted as a secondary event as a side effect of processing a main
    /// event from the pulseaudio mainloop (see `AudioState::update()`)
    SourceProfileChanged { profile_name: Option<String> },
}

impl WasmSerializable for Event {
    fn serialise(self) -> &'static [u8] {
        &[]
    }
}

/// the type of register
/// should match up with `Event` above
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum RegisterType {
    SinksChanged,
    DefaultSinkChanged,

    SourcesChanged,
    DefaultSourceChanged,

    CardsChanged,

    SinkProfileChanged,
    SourceProfileChanged,
}

/// Requests the pulseaudio thread to set properties on the pulseaudio server
// note: requires better docs
#[derive(Debug, Clone)]
pub enum Request {
    /// sets the default sink by sink name (see `Sink.name`)
    SetDefaultSink {
        name: String,
    },
    /// sets the default sink's volume
    //
    // note: could make this take just a f32 and automatically grab
    // the default sink and `ChannelVolumes`
    // as it could take away from the setup thats required elsewhere to make
    // this request
    SetSinkVolume {
        name: String,
        volume: ChannelVolumes,
    },
    SetSinkMute {
        name: String,
        state: bool,
    },

    /// sets the default source by source name (see `Source.name`)
    SetDefaultSource {
        name: String,
    },
    /// sets the default source's volume
    SetSourceVolume {
        name: String,
        volume: ChannelVolumes,
    },

    /// sets the profile of an audio card
    SetCardProfile {
        card_name: String,
        profile_name: String,
    },
}

#[derive(Debug, Clone)]
pub struct AudioState {
    /// used to send requests to the `AudioService`
    pub channel: Sender<ServiceRequest<AudioService>>,
}

impl ServiceChannel<AudioService> for AudioState {
    fn update(&mut self, event: Event) -> Option<Vec<Event>> {
        match event {
            Event::SinksChanged { sinks } => {
                self.sinks = sinks;
            }
            Event::DefaultSinkChanged { name } => {
                self.default_sink = name;

                return self.update_sink_profile();
            }
            Event::SourcesChanged { sources } => {
                self.sources = sources;
            }
            Event::DefaultSourceChanged { name } => {
                self.default_source = name;

                return self.update_source_profile();
            }
            Event::CardsChanged { cards } => {
                self.cards = cards;

                return [self.update_sink_profile(), self.update_source_profile()]
                    .into_iter()
                    .flatten()
                    .flat_map(|v| v)
                    .collect::<Vec<_>>()
                    .into();
            }
            _ => {}
        };

        return None;
    }
}

impl AudioState {
    pub fn get_default_sink(&self) -> Option<Sink> {
        if let Some(sink) = &self.default_sink {
            for s in &self.sinks {
                if sink == &s.name {
                    return Some(s.clone());
                }
            }
        }
        return None;
    }

    pub fn get_default_source(&self) -> Option<Source> {
        if let Some(source) = &self.default_source {
            for s in &self.sources {
                if source == &s.name {
                    return Some(s.clone());
                }
            }
        }
        return None;
    }

    /// `volume` must be between 0.0 - 100.0
    pub fn set_channel_volume(channel: ChannelVolumes, volume: f32) -> ChannelVolumes {
        let vol =
            ((volume / 100.0 * PULSE_MAX_VOLUME as f32).round() as u32).clamp(0, PULSE_MAX_VOLUME);
        // yes this is slower using clone but i wanted it to be clear that the
        // channel volume is being changed, so i didn't use `&mut ChannelVolumes`
        let mut channel = channel.clone();
        channel.set(channel.get().len() as u8, Volume(vol));
        return channel;
    }

    fn set_sink_volume(
        channel: &Sender<ServiceRequest<AudioService>>,
        volume_data: &(String, ChannelVolumes),
    ) {
        if let Err(err) = channel.send(ServiceRequest::Request {
            request: Request::SetSinkVolume {
                name: volume_data.0.clone(),
                volume: volume_data.1,
            },
        }) {
            log::error!(
                "[audio] error while sending Request::SetSinkVolume: {}",
                err
            );
        }
    }

    fn set_source_volume(
        channel: &Sender<ServiceRequest<AudioService>>,
        volume_data: &(String, ChannelVolumes),
    ) {
        if let Err(err) = channel.send(ServiceRequest::Request {
            request: Request::SetSourceVolume {
                name: volume_data.0.clone(),
                volume: volume_data.1,
            },
        }) {
            log::error!(
                "[audio] error while sending Request::SetSourceVolume: {}",
                err
            );
        }
    }

    fn update_sink_profile(&mut self) -> Option<Vec<Event>> {
        let sink: Sink = match self.get_default_sink() {
            Some(sink) => sink,
            None => return None,
        };

        if let Some(index) = sink.card_index {
            for card in &self.cards {
                if index == card.index {
                    self.sink_profiles = card
                        .profiles
                        .iter()
                        .map(|profile| profile.description.clone())
                        .collect::<Vec<String>>();

                    self.sink_default_profile = card
                        .selected_profile
                        .clone()
                        .map(|profile| profile.description);

                    return Some(vec![Event::SinkProfileChanged {
                        profile_name: self.sink_default_profile.clone(),
                    }]);
                }
            }
        }

        return None;
    }

    fn update_source_profile(&mut self) -> Option<Vec<Event>> {
        let source: Source = match self.get_default_source() {
            Some(source) => source,
            None => return None,
        };

        if let Some(index) = source.card_index {
            for card in &self.cards {
                if index == card.index {
                    self.source_profiles = card
                        .profiles
                        .iter()
                        .map(|profile| profile.description.clone())
                        .collect::<Vec<String>>();

                    self.source_default_profile = card
                        .selected_profile
                        .clone()
                        .map(|profile| profile.description);

                    return Some(vec![Event::SourceProfileChanged {
                        profile_name: self.source_default_profile.clone(),
                    }]);
                }
            }
        }

        return None;
    }
}

#[derive(Debug, Clone)]
pub struct AudioService;

impl PassiveService for AudioService {
    type Event = Event;
    type State = AudioState;

    fn subscribe() -> iced::Subscription<ServiceEvent<Self>> {
        let id = TypeId::of::<Self>();

        Subscription::run_with_id(
            id,
            channel(100, async move |mut chan| {
                loop {
                    Self::run(&mut chan).await;
                }
            }),
        )
    }
}

impl ActiveService for AudioService {
    type Request = Request;
    type RegisterData = AudioRegisterData;

    fn request(state: &mut Self::State, request: ServiceRequest<Self>) -> anyhow::Result<()> {}
}

impl AudioService {
    async fn run(chan: &mut IcedSender<ServiceEvent<Self>>) {
        let (request_tx, request_rx) = flume::bounded::<ServiceRequest<Self>>(64);

        // used for communicating with the pulseaudio mainloop
        let (event_tx, event_rx) = flume::bounded::<Event>(64);

        let mut modules_registers: Arc<Mutex<HashMap<RegisterType, Vec<RuntimeModuleId>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        Self::mainloop(Arc::clone(&modules_registers), event_tx, request_rx);

        match chan
            .send(ServiceEvent::Init {
                state: AudioState {
                    channel: request_tx,
                    sink_last_update_time: Instant::now(),
                    sink_will_set_volume: false,
                    sink_volume_data: Arc::new(Mutex::new(None)),
                    source_last_update_time: Instant::now(),
                    source_will_set_volume: false,
                    source_volume_data: Arc::new(Mutex::new(None)),
                    sinks: vec![],
                    default_sink: None,
                    sources: vec![],
                    default_source: None,
                    cards: vec![],
                    sink_profiles: vec![],
                    sink_default_profile: None,
                    source_profiles: vec![],
                    source_default_profile: None,
                },
            })
            .await
        {
            Ok(_) => (),
            Err(err) => {
                log::error!("[audio:subscription] could not send init event: {}", err);
                return;
            }
        };

        'main: loop {
            match event_rx.recv_async().await {
                Ok(event) => {
                    // the scope is used to drop the mutex guard before
                    // awaiting as the guard is not `Send`
                    let ids = {
                        let modules_registers = modules_registers.lock().unwrap();
                        let ids = match &event {
                            Event::SinksChanged { sinks: _ } => {
                                modules_registers.get(&RegisterType::SinksChanged)
                            }
                            Event::DefaultSinkChanged { name: _ } => {
                                modules_registers.get(&RegisterType::DefaultSinkChanged)
                            }
                            Event::SourcesChanged { sources: _ } => {
                                modules_registers.get(&RegisterType::SourcesChanged)
                            }
                            Event::DefaultSourceChanged { name: _ } => {
                                modules_registers.get(&RegisterType::DefaultSourceChanged)
                            }
                            Event::CardsChanged { cards: _ } => {
                                modules_registers.get(&RegisterType::CardsChanged)
                            }
                            Event::SinkProfileChanged { profile_name: _ } => {
                                modules_registers.get(&RegisterType::SinkProfileChanged)
                            }
                            Event::SourceProfileChanged { profile_name: _ } => {
                                modules_registers.get(&RegisterType::SourceProfileChanged)
                            }
                        };

                        match ids {
                            Some(ids) => ids,
                            None => continue 'main,
                        }
                        .clone()
                    };

                    for id in ids {
                        match chan
                            .send(ServiceEvent::Update {
                                id,
                                event: event.clone(),
                            })
                            .await
                        {
                            Ok(_) => (),
                            Err(err) => {
                                log::error!(
                                    "[audio:subscription] error sending service event update: {err}"
                                );
                                continue 'main;
                            }
                        };
                    }
                }
                Err(err) => {
                    log::error!(
                        "[audio:subscription] error receiving message from mainloop: {err}"
                    );
                    continue 'main;
                }
            };
        }
    }

    /// initialize mainloop for later setup
    ///
    /// returns the mainloop and context
    ///
    /// todo: return custom error or anyhow to explain error
    pub fn init_mainloop() -> anyhow::Result<(Mainloop, Context)> {
        let mut proplist = Proplist::new().unwrap();
        proplist
            .set_str(properties::APPLICATION_NAME, "aurorashell")
            .unwrap();

        let mut mainloop = Mainloop::new().expect("error creating pulseaudio mainloop :3");

        let mut context = Context::new_with_proplist(&mainloop, "aurorashell", &proplist)
            .expect("Failed to create context");

        context
            .connect(None, FlagSet::NOFLAGS, None)
            .expect("Failed to connect to PulseAudio");

        // wait for context to be ready
        loop {
            match mainloop.iterate(false) {
                IterateResult::Quit(_) | IterateResult::Err(_) => {
                    return Err(anyhow::format_err!(
                        "failed to iterate while waiting for context ready"
                    ));
                }
                IterateResult::Success(_) => {}
            }

            match context.get_state() {
                pulse::context::State::Ready => {
                    break;
                }
                pulse::context::State::Failed | pulse::context::State::Terminated => {
                    log::error!("[audio] context failed to ready");
                    return Err(anyhow::format_err!("failed to start mainloop loop"));
                }
                _ => {}
            }
        }

        return Ok((mainloop, context));
    }

    /// spawns a thread for the synchronous pulseaudio mainloop
    ///
    /// this code can't be part of `Self:run` as the pulseaudio mainloop
    /// doesn't like async
    fn mainloop(
        module_registers: Arc<Mutex<HashMap<RegisterType, Vec<RuntimeModuleId>>>>,
        event_tx: Sender<Event>,
        request_rx: Receiver<ServiceRequest<Self>>,
    ) {
        // handle events
        thread::spawn(move || {
            let (mut mainloop, mut context) = match Self::init_mainloop() {
                Ok(res) => res,
                Err(err) => {
                    log::error!("[audio] error while creating mainloop: {err}");
                    return;
                }
            };

            let introspector = context.introspect();

            get_sinks(&introspector, event_tx.clone());
            get_sources(&introspector, event_tx.clone());
            get_default_devices(&introspector, event_tx.clone());
            get_cards(&introspector, event_tx.clone());

            let interest_mask = InterestMaskSet::SERVER
                | InterestMaskSet::CLIENT
                | InterestMaskSet::SOURCE
                | InterestMaskSet::SINK
                | InterestMaskSet::CARD;

            context.subscribe(interest_mask, |success| {
                log::debug!("[audio] subscribe success: {success}");
            });

            context.set_subscribe_callback(Some(Box::new(move |facility, operation, _index| {
                let event_type = facility.unwrap();
                let event = operation.unwrap();

                log::debug!("[audio] {event_type:?}, {event:?}");

                match event {
                    subscribe::Operation::New => {
                        match event_type {
                            subscribe::Facility::Sink => {
                                get_sinks(&introspector, event_tx.clone());
                            }
                            subscribe::Facility::Source => {
                                get_sources(&introspector, event_tx.clone());
                            }
                            _ => (),
                        };
                    }
                    subscribe::Operation::Removed => {
                        match event_type {
                            subscribe::Facility::Sink => {
                                get_sinks(&introspector, event_tx.clone());
                            }
                            subscribe::Facility::Source => {
                                get_sources(&introspector, event_tx.clone());
                            }
                            _ => (),
                        };
                    }
                    subscribe::Operation::Changed => {
                        match event_type {
                            subscribe::Facility::Server => {
                                get_default_devices(&introspector, event_tx.clone());
                            }
                            subscribe::Facility::Card => {
                                get_cards(&introspector, event_tx.clone());
                            }
                            subscribe::Facility::Sink => {
                                get_sinks(&introspector, event_tx.clone());
                            }
                            subscribe::Facility::Source => {
                                get_sources(&introspector, event_tx.clone());
                            }
                            _ => (),
                        };
                    }
                }
            })));

            loop {
                let result = mainloop.iterate(true);
                match result {
                    IterateResult::Quit(q) => {
                        // note: shouldn't panic here but idrc for now :3
                        // gracefully attempt a service restart
                        log::error!("[audio] [pulseaudio thread 1] [PANIC] mainloop quit: {q:?}");
                        panic!();
                    }
                    IterateResult::Err(e) => {
                        // note: need to only allow errors a few times then
                        // restart the service
                        //
                        // or: restart the service immediately
                        log::error!(
                            "[audio] [pulseaudio thread 1] [PANIC] [audio] mainloop error: {e}"
                        );
                    }
                    _ => {}
                };
            }
        });

        // handles requests
        thread::spawn(move || {
            let (mut mainloop, mut context) = match Self::init_mainloop() {
                Ok(res) => res,
                Err(err) => {
                    log::error!("[audio] error while creating mainloop: {err}");
                    return;
                }
            };

            loop {
                let result = match request_rx.recv() {
                    Ok(res) => res,
                    Err(err) => {
                        log::warn!(
                            "[audio] [pulseaudio thread 2] could not receive request for mainloop, error: {err}"
                        );
                        log::warn!("[audio] [pulseaudio thread 2] retrying in 5 seconds...");
                        thread::sleep(Duration::from_secs(5));
                        continue;
                    }
                };

                // note: remove before push :3
                log::debug!("*nuzzles u like the good girl fox i am* >:3");

                match result {
                    ServiceRequest::RegisterModule { id, data } => {
                        // note: add actual tracing library
                        log::trace!("[audio] Request::RegisterModule = {:?}, {:?}", id, data);

                        let mut module_registers = module_registers.lock().unwrap();
                        if data.is_set(AudioRegisterData::SINKS_CHANGED) {
                            match module_registers.get_mut(&RegisterType::SinksChanged) {
                                Some(ids) => ids.push(id),
                                None => {
                                    module_registers.insert(RegisterType::SinksChanged, vec![id]);
                                }
                            };
                        }
                        if data.is_set(AudioRegisterData::DEFAULT_SINK_CHANGED) {
                            match module_registers.get_mut(&RegisterType::DefaultSinkChanged) {
                                Some(ids) => ids.push(id),
                                None => {
                                    module_registers
                                        .insert(RegisterType::DefaultSinkChanged, vec![id]);
                                }
                            };
                        }
                        if data.is_set(AudioRegisterData::SOURCES_CHANGED) {
                            match module_registers.get_mut(&RegisterType::SourcesChanged) {
                                Some(ids) => ids.push(id),
                                None => {
                                    module_registers.insert(RegisterType::SourcesChanged, vec![id]);
                                }
                            };
                        }
                        if data.is_set(AudioRegisterData::DEFAULT_SOURCE_CHANGED) {
                            match module_registers.get_mut(&RegisterType::DefaultSourceChanged) {
                                Some(ids) => ids.push(id),
                                None => {
                                    module_registers
                                        .insert(RegisterType::DefaultSourceChanged, vec![id]);
                                }
                            };
                        }
                        if data.is_set(AudioRegisterData::CARDS_CHANGED) {
                            match module_registers.get_mut(&RegisterType::CardsChanged) {
                                Some(ids) => ids.push(id),
                                None => {
                                    module_registers.insert(RegisterType::CardsChanged, vec![id]);
                                }
                            };
                        }
                        if data.is_set(AudioRegisterData::SINK_PROFILE_CHANGED) {
                            match module_registers.get_mut(&RegisterType::SinkProfileChanged) {
                                Some(ids) => ids.push(id),
                                None => {
                                    module_registers.insert(RegisterType::CardsChanged, vec![id]);
                                }
                            };
                        }
                        if data.is_set(AudioRegisterData::SOURCE_PROFILE_CHANGED) {
                            match module_registers.get_mut(&RegisterType::SourceProfileChanged) {
                                Some(ids) => ids.push(id),
                                None => {
                                    module_registers
                                        .insert(RegisterType::SourceProfileChanged, vec![id]);
                                }
                            };
                        }
                    }
                    ServiceRequest::Request { request } => match request {
                        Request::SetDefaultSink { name } => {
                            context.set_default_sink(name.as_str(), |_| {});
                        }
                        Request::SetSinkVolume { name, volume } => {
                            context.introspect().set_sink_volume_by_name(
                                name.as_str(),
                                &volume,
                                None,
                            );
                        }
                        Request::SetSinkMute { name, state } => {
                            context
                                .introspect()
                                .set_sink_mute_by_name(name.as_str(), state, None);
                        }
                        Request::SetDefaultSource { name } => {
                            context.set_default_source(name.as_str(), |_| {});
                        }
                        Request::SetSourceVolume { name, volume } => {
                            context.introspect().set_source_volume_by_name(
                                name.as_str(),
                                &volume,
                                None,
                            );
                        }
                        Request::SetCardProfile {
                            card_name,
                            profile_name,
                        } => {
                            context.introspect().set_card_profile_by_name(
                                card_name.as_str(),
                                profile_name.as_str(),
                                None,
                            );
                        }
                    },
                };

                let result = mainloop.iterate(true);
                match result {
                    IterateResult::Quit(q) => {
                        // probably shouldn't panic here but idrc for now :3
                        panic!("[audio] [pulseaudio thread 2] mainloop quit: {q:?}");
                    }
                    IterateResult::Err(e) => {
                        panic!("[audio] [pulseaudio thread 2] mainloop error: {e}");
                    }
                    _ => {}
                };
            }
        });
    }
}
