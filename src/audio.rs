use std::borrow::Cow;
use std::sync::{Arc, RwLock};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use pulse::callbacks::ListResult;
use pulse::context::introspect::Introspector;
use pulse::context::subscribe::{self, InterestMaskSet};
use pulse::context::{Context, FlagSet};
use pulse::mainloop::standard::{IterateResult, Mainloop};
use pulse::proplist::{Proplist, properties};
use pulse::volume::ChannelVolumes;

#[derive(Debug, Clone)]
pub enum Message {
    SinksChanged(Vec<Sink>),
    DefaultSinkChanged(Option<String>),

    SourcesChanged(Vec<Source>),
    DefaultSourceChanged(Option<String>),

    CardsChanged(Vec<Card>),
}

/// Requests the pulseaudio thread to set properties on the pulseaudio server
#[derive(Debug, Clone)]
pub enum Request {
    SetDefaultSink(String),
    SetSinkVolume(String, ChannelVolumes),
    SetSinkMute(String, bool),

    SetDefaultSource(String),
    SetSourceVolume(String, ChannelVolumes),

    SetCardProfile(String, String),
}

/// Initialize mainloop for later setup
///
/// Returns the mainloop and context
///
/// todo: return custom error or anyhow to explain error
pub fn init() -> anyhow::Result<(Mainloop, Context)> {
    let mut proplist = Proplist::new().unwrap();
    proplist
        .set_str(properties::APPLICATION_NAME, "AuroraAudioWidget")
        .unwrap();

    let mut mainloop = Mainloop::new().expect("error creating pulseaudio mainloop :3");

    let mut context = Context::new_with_proplist(&mainloop, "AuroraAudioWidget", &proplist)
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
                eprintln!("context failed to ready");
                return Err(anyhow::format_err!("failed to start mainloop loop"));
            }
            _ => {}
        }
    }

    return Ok((mainloop, context));
}

/// Call after `Self::init`
///
/// Setup and run mainloop hooks
///
/// Returns an optional channel to receive messages from the callbacks
/// and a handle to the thread handling those messages if setup went well! :3
pub fn run() -> anyhow::Result<(
    JoinHandle<()>,
    flume::Sender<Request>,
    flume::Receiver<Message>,
)> {
    let (tx, audio_rx) = flume::unbounded::<Message>();
    let (audio_tx, rx) = flume::unbounded::<Request>();

    // used so the thread can signal if it failed to start
    let (is_ok_tx, is_ok_rx) = flume::bounded::<bool>(1);

    let handle = thread::spawn(move || {
        let (mut mainloop, mut context) = match init() {
            Ok(res) => res,
            Err(err) => {
                eprintln!("[pulseaudio thread 1] error while creating mainloop: {err}");
                match is_ok_tx.send(false) {
                    Ok(_) => {}
                    Err(err) => {
                        eprintln!("[pulseaudio thread 1] error sending failure: {err}");
                    }
                };
                return;
            }
        };

        let introspector = context.introspect();

        get_sinks(&introspector, tx.clone());
        get_sources(&introspector, tx.clone());
        get_default_devices(&introspector, tx.clone());
        get_cards(&introspector, tx.clone());

        let interest_mask = InterestMaskSet::SERVER
            | InterestMaskSet::CLIENT
            | InterestMaskSet::SOURCE
            | InterestMaskSet::SINK
            | InterestMaskSet::CARD;

        context.subscribe(interest_mask, |success| {
            println!("Subscribe success: {success}");
        });

        context.set_subscribe_callback(Some(Box::new(move |facility, operation, _index| {
            let event_type = facility.unwrap();
            let event = operation.unwrap();

            let timestamp = chrono::Utc::now().format("%H:%M:%S.%f");
            //println!("[{timestamp}] {event_type:?}, {event:?}");

            match event {
                subscribe::Operation::New => {
                    match event_type {
                        subscribe::Facility::Sink => {
                            get_sinks(&introspector, tx.clone());
                        }
                        subscribe::Facility::Source => {
                            get_sources(&introspector, tx.clone());
                        }
                        _ => (),
                    };
                }
                subscribe::Operation::Removed => {
                    match event_type {
                        subscribe::Facility::Sink => {
                            get_sinks(&introspector, tx.clone());
                        }
                        subscribe::Facility::Source => {
                            get_sources(&introspector, tx.clone());
                        }
                        _ => (),
                    };
                }
                subscribe::Operation::Changed => {
                    match event_type {
                        subscribe::Facility::Server => {
                            get_default_devices(&introspector, tx.clone());
                        }
                        subscribe::Facility::Card => {
                            println!("*fox giggle* >:3");
                            get_cards(&introspector, tx.clone());
                        }
                        subscribe::Facility::Sink => {
                            get_sinks(&introspector, tx.clone());
                        }
                        subscribe::Facility::Source => {
                            get_sources(&introspector, tx.clone());
                        }
                        _ => (),
                    };
                }
            }
        })));

        match is_ok_tx.send(true) {
            Ok(_) => {}
            Err(err) => {
                eprintln!("[pulseaudio thread 1] error while sending success: {err}");
            }
        }

        loop {
            let result = mainloop.iterate(true);
            match result {
                IterateResult::Quit(q) => {
                    // probably shouldn't panic here but idrc for now :3
                    panic!("[pulseaudio thread 1] mainloop quit: {q:?}");
                }
                IterateResult::Err(e) => {
                    panic!("[pulseaudio thread 1] mainloop error: {e}");
                }
                _ => {}
            };
        }
    });

    match is_ok_rx.recv_timeout(Duration::from_millis(1000)) {
        Ok(success) => match success {
            true => (),
            false => return Err(anyhow::format_err!("pulseaudio thread 1 failed to start")),
        },
        Err(err) => return Err(err.into()),
    };

    // used so the thread can signal if it failed to start
    let (is_ok_tx, is_ok_rx) = flume::bounded::<bool>(1);

    // second thread for receiving commands
    // to send to the pulseaudio server
    thread::spawn(move || {
        let (mut mainloop, mut context) = match init() {
            Ok(res) => res,
            Err(err) => {
                eprintln!("[pulseaudio thread 2] error while creating mainloop: {err}");
                match is_ok_tx.send(false) {
                    Ok(_) => {}
                    Err(err) => {
                        eprintln!("[pulseaudio thread 2] error while sending failure: {err}");
                    }
                };
                return;
            }
        };

        // note(aurora): maybe not the best place for this
        match is_ok_tx.send(true) {
            Ok(_) => {}
            Err(err) => {
                eprintln!("[pulseaudio thread 2] error while sending success: {err}");
            }
        }

        loop {
            let result = match rx.recv() {
                Ok(res) => res,
                Err(err) => {
                    eprintln!(
                        "[pulseaudio thread 2] could not receive request for mainloop, error: {err}"
                    );
                    continue;
                }
            };

            println!("*nuzzles u like the good girl fox i am* >:3");

            match result {
                Request::SetDefaultSink(sink) => {
                    context.set_default_sink(sink.as_str(), |_| {});
                }
                Request::SetSinkVolume(sink, volume) => {
                    context
                        .introspect()
                        .set_sink_volume_by_name(sink.as_str(), &volume, None);
                }
                Request::SetSinkMute(sink, mute) => {
                    context
                        .introspect()
                        .set_sink_mute_by_name(sink.as_str(), mute, None);
                }
                Request::SetDefaultSource(source) => {
                    context.set_default_source(source.as_str(), |_| {});
                }
                Request::SetSourceVolume(sink, volume) => {
                    context
                        .introspect()
                        .set_source_volume_by_name(sink.as_str(), &volume, None);
                }
                Request::SetCardProfile(card, profile) => {
                    context.introspect().set_card_profile_by_name(
                        card.as_str(),
                        profile.as_str(),
                        None,
                    );
                }
            };

            let result = mainloop.iterate(true);
            match result {
                IterateResult::Quit(q) => {
                    // probably shouldn't panic here but idrc for now :3
                    panic!("[pulseaudio thread 2] mainloop quit: {q:?}");
                }
                IterateResult::Err(e) => {
                    panic!("[pulseaudio thread 2] mainloop error: {e}");
                }
                _ => {}
            };
        }
    });

    return match is_ok_rx.recv_timeout(Duration::from_millis(1000)) {
        Ok(success) => match success {
            true => Ok((handle, audio_tx, audio_rx)),
            false => Err(anyhow::format_err!("thread failed to start")),
        },
        Err(err) => Err(err.into()),
    };
}

#[derive(Debug, Clone)]
pub struct Sink {
    pub name: String,
    pub description: String,
    pub volume: ChannelVolumes,
    pub mute: bool,
    pub card_index: Option<u32>,
}

impl PartialEq for Sink {
    fn eq(&self, other: &Self) -> bool {
        return self.name == other.name
            && self.description == other.description
            && self.volume.get() == other.volume.get()
            && self.mute == other.mute
            && self.card_index == other.card_index;
    }
}

fn get_sinks(introspector: &Introspector, tx: flume::Sender<Message>) {
    let sinks = Arc::new(RwLock::new(Vec::<Sink>::new()));
    let sinks_ref = Arc::clone(&sinks);

    // used so the thread can signal if it failed to start
    let (is_ok_tx, is_ok_rx) = flume::unbounded::<bool>();

    introspector.get_sink_info_list(move |sink_info| match sink_info {
        ListResult::Item(sink) => {
            let sink_x3 = Sink {
                name: sink.name.clone().unwrap().to_string(),
                description: sink
                    .description
                    .clone()
                    .unwrap_or(Cow::Borrowed("Unknown"))
                    .to_string(),
                volume: sink.volume,
                mute: sink.mute,
                card_index: sink.card,
            };

            sinks_ref.write().unwrap().push(sink_x3);
        }
        ListResult::End => {
            if let Err(err) = is_ok_tx.send(true) {
                eprintln!(
                    "error while sending success for introspector.get_sink_info_list: {}",
                    err
                );
            }
        }
        ListResult::Error => {
            eprintln!("error while processing introspector.get_sink_info_list");
            if let Err(err) = is_ok_tx.send(false) {
                eprintln!(
                    "error while sending failure for introspector.get_sink_info_list: {}",
                    err
                );
            }
        }
    });

    thread::spawn(move || {
        match is_ok_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(success) => match success {
                true => {
                    if let Err(err) = tx.send(Message::SinksChanged(sinks.read().unwrap().to_vec()))
                    {
                        eprintln!("error while sending Message::SinksChanged: {err}");
                    }
                }
                false => {
                    eprintln!("error while getting sinks")
                }
            },
            Err(err) => {
                eprintln!(
                    "error while waiting for introspector.get_sink_info_list: {}",
                    err
                );
            }
        };
    });
}

#[derive(Debug, Clone)]
pub struct Source {
    pub name: String,
    pub description: String,
    pub volume: ChannelVolumes,
}

impl PartialEq for Source {
    fn eq(&self, other: &Self) -> bool {
        return self.name == other.name
            && self.description == other.description
            && self.volume.get() == other.volume.get();
    }
}

fn get_sources(introspector: &Introspector, tx: flume::Sender<Message>) {
    let sources = Arc::new(RwLock::new(Vec::<Source>::new()));
    let sources_ref = Arc::clone(&sources);

    // used so the thread can signal if it failed to start
    let (is_ok_tx, is_ok_rx) = flume::unbounded::<bool>();

    introspector.get_source_info_list(move |source_info| match source_info {
        ListResult::Item(source) => {
            // don't get monitors
            // todo?: maybe allow user to select monitors
            if let None = source.monitor_of_sink {
                let source_x3 = Source {
                    name: source.name.clone().unwrap().to_string(),
                    description: source
                        .description
                        .clone()
                        .unwrap_or(Cow::Borrowed("Unknown"))
                        .to_string(),
                    volume: source.volume,
                };

                sources_ref.write().unwrap().push(source_x3);
            }
        }
        ListResult::End => {
            if let Err(err) = is_ok_tx.send(true) {
                eprintln!(
                    "error while sending success for introspector.get_source_info_list: {}",
                    err
                );
            }
        }
        ListResult::Error => {
            eprintln!("error while processing introspector.get_source_info_list");
            if let Err(err) = is_ok_tx.send(false) {
                eprintln!(
                    "error while sending failure for introspector.get_source_info_list: {}",
                    err
                );
            }
        }
    });

    thread::spawn(move || {
        match is_ok_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(success) => match success {
                true => {
                    if let Err(err) =
                        tx.send(Message::SourcesChanged(sources.read().unwrap().to_vec()))
                    {
                        eprintln!("error while sending Message::SourcesChanged: {err}");
                    }
                }
                false => {
                    eprintln!("error while getting sources")
                }
            },
            Err(err) => {
                eprintln!(
                    "error while waiting for introspector.get_source_info_list: {}",
                    err
                );
            }
        };
    });
}

fn get_default_devices(introspector: &Introspector, tx: flume::Sender<Message>) {
    let default_sink: Arc<RwLock<Option<String>>> = Arc::new(RwLock::new(None));
    let default_source: Arc<RwLock<Option<String>>> = Arc::new(RwLock::new(None));
    let default_sink_ref = Arc::clone(&default_sink);
    let default_source_ref = Arc::clone(&default_source);

    // used so the thread can signal if it failed to start
    let (is_ok_tx, is_ok_rx) = flume::bounded::<bool>(1);

    introspector.get_server_info(move |server_info| {
        println!("sink = {:?}", server_info.default_sink_name);
        println!("source = {:?}", server_info.default_source_name);

        let mut sink = default_sink_ref
            .write()
            .expect("default sink rwlock poisioned");
        *sink = match &server_info.default_sink_name {
            Some(sink) => Some(sink.to_string()),
            None => None,
        };

        let mut source = default_source_ref
            .write()
            .expect("default source rwlock poisioned");
        *source = match &server_info.default_source_name {
            Some(source) => Some(source.to_string()),
            None => None,
        };

        match is_ok_tx.send(true) {
            Ok(_) => {}
            Err(err) => {
                eprintln!(
                    "error while sending success for introspector.get_server_info: {}",
                    err
                );
            }
        };
    });

    thread::spawn(move || {
        match is_ok_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(success) => match success {
                true => {
                    println!("LOOK! ANOTHER FOX LIKE ME! *points happily with tail wagging*");

                    if let Err(err) = tx.send(Message::DefaultSinkChanged(
                        default_sink.read().unwrap().clone(),
                    )) {
                        eprintln!("error while sending Message::DefaultSinkChanged: {err}");
                    }

                    if let Err(err) = tx.send(Message::DefaultSourceChanged(
                        default_source.read().unwrap().clone(),
                    )) {
                        eprintln!("error while sending Message::DefaultSinkChanged: {err}");
                    }
                }
                false => {
                    eprintln!("error while getting default sink and source")
                }
            },
            Err(err) => {
                eprintln!(
                    "error while waiting for introspector.get_server_info: {}",
                    err
                );
            }
        };
    });
}

#[derive(Debug, Clone, PartialEq)]
pub struct Card {
    pub name: String,
    pub index: u32,
    pub profiles: Vec<Profile>,
    pub selected_profile: Option<Profile>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Profile {
    pub name: String,
    pub description: String,
}

fn get_cards(introspector: &Introspector, tx: flume::Sender<Message>) {
    let cards = Arc::new(RwLock::new(Vec::<Card>::new()));
    let cards_ref = Arc::clone(&cards);

    // used so the thread can signal if it failed to start
    let (is_ok_tx, is_ok_rx) = flume::bounded::<bool>(1);

    introspector.get_card_info_list(move |card_info| match card_info {
        ListResult::Item(card) => {
            let card_x3 = Card {
                name: card.name.clone().unwrap().to_string(),
                index: card.index,
                profiles: card
                    .profiles
                    .iter()
                    .map(|profile| Profile {
                        name: profile.name.clone().unwrap().to_string(),
                        description: profile
                            .description
                            .clone()
                            .unwrap_or(Cow::Borrowed("Unknown"))
                            .to_string(),
                    })
                    .collect::<Vec<Profile>>(),
                selected_profile: card.active_profile.as_ref().map(|profile| Profile {
                    name: profile.name.clone().unwrap().to_string(),
                    description: profile
                        .description
                        .clone()
                        .unwrap_or(Cow::Borrowed("Unknown"))
                        .to_string(),
                }),
            };

            cards_ref.write().unwrap().push(card_x3);
        }
        ListResult::End => {
            if let Err(err) = is_ok_tx.send(true) {
                eprintln!(
                    "error while sending success for introspector.get_card_info_list: {}",
                    err
                );
            }
        }
        ListResult::Error => {
            eprintln!("error while processing introspector.get_card_info_list");
            if let Err(err) = is_ok_tx.send(false) {
                eprintln!(
                    "error while sending failure for introspector.get_card_info_list: {}",
                    err
                );
            }
        }
    });

    thread::spawn(move || {
        match is_ok_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(success) => match success {
                true => {
                    if let Err(err) = tx.send(Message::CardsChanged(cards.read().unwrap().clone()))
                    {
                        eprintln!("error while sending Message::CardsChanged: {err}");
                    }
                }
                false => {
                    eprintln!("error while getting cards");
                }
            },
            Err(err) => {
                eprintln!(
                    "error while waiting for introspector.get_card_info_list: {}",
                    err
                );
            }
        };
    });
}
