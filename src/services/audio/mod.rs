mod messages;
mod se;
mod state;
mod wasm;

use messages::{Event, Request, get_cards, get_default_devices, get_sinks, get_sources};
use state::AudioRequestThreadState;

use crate::runtime::module::AudioRegisterData;
use crate::services::{Service, ServiceEvent, ServiceRequest, ServiceState};

use std::any::TypeId;
use std::thread;
use std::time::Duration;

use anyhow::anyhow;
use iced::Subscription;
use iced::futures::SinkExt;
use iced::futures::channel::mpsc;
use iced::stream::channel;
use pulse::context::subscribe::{
    InterestMaskSet, {self},
};
use pulse::context::{Context, FlagSet};
use pulse::mainloop::standard::{IterateResult, Mainloop};
use pulse::proplist::{Proplist, properties};
use state::AudioState;

////////////////////////////////////////////////////////////////////////////////
// service parameters

/// configures the capacity for all channels in this service
const CHANNEL_CAPACITY: usize = 64;

/// the time between requesting pulseaudio to update a value that can
/// be frequently updated (like volume) to stop us from spamming the server
/// which is visually laggy
const UPDATE_INTERVAL: Duration = Duration::from_millis(100);

/// 65536 represents 100% in pulseaudio
///
/// this constant sets the maximum possible volume that we allow
///
/// note: this can be removed at some point or a more sensible limit can be
/// found as the Volume Control app allows up to 153% volume
pub const PULSE_MAX_VOLUME: u32 = 65536;

////////////////////////////////////////////////////////////////////////////////

#[derive(Debug, Clone)]
pub struct AudioService;

impl Service for AudioService {
    type Event = Event;
    type RegisterData = AudioRegisterData;
    type Request = Request;
    type RuntimeData = (AudioRequestThreadState,);
    type State = AudioState;

    fn subscribe() -> iced::Subscription<ServiceEvent<Self>> {
        let id = TypeId::of::<Self>();

        Subscription::run_with_id(
            id,
            channel(CHANNEL_CAPACITY, async |mut chan| {
                loop {
                    let mut state = AudioState::init();

                    // setup channel for modules to be able to talk to this
                    // service :3
                    let (tx, rx) = flume::bounded::<ServiceRequest<Self>>(CHANNEL_CAPACITY);

                    if let Err(err) = chan
                        .send(ServiceEvent::Init {
                            request_tx: tx.clone(),
                        })
                        .await
                    {
                        log::error!("[service:audio] could not send init event: {}", err);
                        log::error!("[service:audio] retrying in 5 seconds...");
                        tokio::time::sleep(Duration::from_secs(5)).await;
                        continue;
                    }

                    let mut runtime_data = (AudioRequestThreadState::init(tx),);

                    let err = Self::run(&mut state, &mut chan, rx, &mut runtime_data).await;
                    log::error!("[service:audio] mainloop error: {err}");
                }
            }),
        )
    }

    async fn run(
        state: &mut AudioState,
        chan: &mut mpsc::Sender<ServiceEvent<Self>>,
        request_rx: flume::Receiver<ServiceRequest<Self>>,
        runtime_data: &mut (AudioRequestThreadState,),
    ) -> anyhow::Error {
        log::info!("[service:audio] service started!");

        // used for communicating with the pulseaudio mainloop
        // as i haven't found a way to use the async channels that are already
        // provided by the subscription in the mainloop part
        let (event_tx, event_rx) = flume::bounded::<Event>(CHANNEL_CAPACITY);

        let (request_state,) = runtime_data;

        Self::mainloop(event_tx, request_rx, request_state.clone());

        loop {
            match event_rx.recv_async().await {
                Ok(event) => {
                    let events = state.update(event.clone());
                    log::debug!("{:?}", events); // note: prob remove this, not needed

                    for event in events {
                        match chan.send(ServiceEvent::Update { event }).await {
                            Ok(_) => {}
                            Err(err) => {
                                log::error!(
                                    "[service:audio] error sending service event update: {err}"
                                );
                                continue;
                            }
                        }
                    }
                }
                Err(err) => {
                    return anyhow!("[service:audio] error receiving message from mainloop: {err}");
                }
            }
        }
    }
}

impl AudioService {
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
    /// this code can't be part of `Self::run` as the pulseaudio mainloop
    /// doesn't like async
    fn mainloop(
        event_tx: flume::Sender<Event>,
        request_rx: flume::Receiver<ServiceRequest<Self>>,
        mut request_state: AudioRequestThreadState,
    ) {
        // thread to handle events to modules
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

        // thread to handle requests from modules
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
                            "[audio] [pulseaudio thread 2] could not receive request for \
                             mainloop, error: {err}"
                        );
                        log::warn!("[audio] [pulseaudio thread 2] retrying in 5 seconds...");
                        thread::sleep(Duration::from_secs(5));
                        continue;
                    }
                };

                match result {
                    ServiceRequest::Request { request } => match request {
                        Request::SetDefaultSink { name } => {
                            context.set_default_sink(name.as_str(), |_| {});
                        }
                        Request::SetSinkVolume { name, volume } => {
                            if request_state.set_sink_volume(name.clone(), volume.clone()) {
                                context.introspect().set_sink_volume_by_name(
                                    name.as_str(),
                                    &volume,
                                    None,
                                );
                            }
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
