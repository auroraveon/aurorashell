use std::borrow::Cow;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use pulse::callbacks::ListResult;
use pulse::context::introspect::Introspector;
use pulse::volume::ChannelVolumes;

/// messages emitted from the audio service when an event happens
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

/// requests the pulseaudio thread to set properties on the pulseaudio server
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

////////////////////////////////////////////////////////////////////////////////
// event type

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum AudioEventType {
    SinksChanged,
    DefaultSinkChanged,
    SourcesChanged,
    DefaultSourceChanged,
    CardsChanged,
    SinkProfileChanged,
    SourceProfileChanged,
}

////////////////////////////////////////////////////////////////////////////////
// types used for events

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

pub fn get_sinks(introspector: &Introspector, chan: flume::Sender<Event>) {
    let sinks = Arc::new(Mutex::new(Vec::<Sink>::new()));
    let sinks_ref = Arc::clone(&sinks);

    // used so the thread can signal if it failed to start
    let (tx, rx) = flume::bounded::<bool>(1);

    introspector.get_sink_info_list(move |sink_info| match sink_info {
        ListResult::Item(sink) => {
            let sink = Sink {
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

            sinks_ref.lock().unwrap().push(sink);
        }
        ListResult::End => {
            if let Err(err) = tx.send(true) {
                log::error!(
                    "error while sending success for introspector.get_sink_info_list: {}",
                    err
                );
            }
        }
        ListResult::Error => {
            log::warn!("could not process introspector.get_sink_info_list");
            if let Err(err) = tx.send(false) {
                log::error!(
                    "error while sending failure for introspector.get_sink_info_list: {}",
                    err
                );
            }
        }
    });

    thread::spawn(move || {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(success) => match success {
                true => {
                    let data = {
                        let guard = sinks.lock().unwrap();
                        guard.to_vec()
                    };

                    if let Err(err) = chan.send(Event::SinksChanged { sinks: data }) {
                        log::error!("error while sending Message::SinksChanged: {err}");
                    }
                }
                false => {
                    log::warn!("could not get sinks")
                }
            },
            Err(err) => {
                log::error!(
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
    pub mute: bool,
    pub card_index: Option<u32>,
}

impl PartialEq for Source {
    fn eq(&self, other: &Self) -> bool {
        return self.name == other.name
            && self.description == other.description
            && self.volume.get() == other.volume.get();
    }
}

pub fn get_sources(introspector: &Introspector, chan: flume::Sender<Event>) {
    let sources = Arc::new(Mutex::new(Vec::<Source>::new()));
    let sources_ref = Arc::clone(&sources);

    // used so the thread can signal if it failed to start
    let (tx, rx) = flume::bounded::<bool>(1);

    introspector.get_source_info_list(move |source_info| match source_info {
        ListResult::Item(source) => {
            // don't get monitors
            // todo?: maybe allow user to select monitors
            if let None = source.monitor_of_sink {
                let source = Source {
                    name: source.name.clone().unwrap().to_string(),
                    description: source
                        .description
                        .clone()
                        .unwrap_or(Cow::Borrowed("Unknown"))
                        .to_string(),
                    volume: source.volume,
                    mute: source.mute,
                    card_index: source.card,
                };

                sources_ref.lock().unwrap().push(source);
            }
        }
        ListResult::End => {
            if let Err(err) = tx.send(true) {
                log::error!(
                    "error while sending success for introspector.get_source_info_list: {}",
                    err
                );
            }
        }
        ListResult::Error => {
            log::warn!("could not process introspector.get_source_info_list");
            if let Err(err) = tx.send(false) {
                log::error!(
                    "error while sending failure for introspector.get_source_info_list: {}",
                    err
                );
            }
        }
    });

    thread::spawn(move || {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(success) => match success {
                true => {
                    let data = {
                        let guard = sources.lock().unwrap();
                        guard.to_vec()
                    };
                    if let Err(err) = chan.send(Event::SourcesChanged { sources: data }) {
                        log::error!("error while sending Message::SourcesChanged: {err}");
                    }
                }
                false => {
                    log::warn!("could not get sources")
                }
            },
            Err(err) => {
                log::error!(
                    "error while waiting for introspector.get_source_info_list: {}",
                    err
                );
            }
        };
    });
}

pub fn get_default_devices(introspector: &Introspector, chan: flume::Sender<Event>) {
    let default_sink: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let default_source: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let default_sink_ref = Arc::clone(&default_sink);
    let default_source_ref = Arc::clone(&default_source);

    // used so the thread can signal if it failed to start
    let (tx, rx) = flume::bounded::<bool>(1);

    introspector.get_server_info(move |server_info| {
        let mut sink = default_sink_ref
            .lock()
            .expect("default sink rwlock poisioned");
        *sink = match &server_info.default_sink_name {
            Some(sink) => Some(sink.to_string()),
            None => None,
        };

        let mut source = default_source_ref
            .lock()
            .expect("default source rwlock poisioned");
        *source = match &server_info.default_source_name {
            Some(source) => Some(source.to_string()),
            None => None,
        };

        match tx.send(true) {
            Ok(_) => {}
            Err(err) => {
                log::error!(
                    "error while sending success for introspector.get_server_info: {}",
                    err
                );
            }
        };
    });

    thread::spawn(move || {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(success) => match success {
                true => {
                    let data = {
                        let guard = default_sink.lock().unwrap();
                        guard.clone()
                    };

                    if let Err(err) = chan.send(Event::DefaultSinkChanged { name: data }) {
                        log::error!("error while sending Message::DefaultSinkChanged: {err}");
                    }

                    let data = {
                        let guard = default_source.lock().unwrap();
                        guard.clone()
                    };

                    if let Err(err) = chan.send(Event::DefaultSourceChanged { name: data }) {
                        log::error!("error while sending Message::DefaultSinkChanged: {err}");
                    }
                }
                false => {
                    log::warn!("could not get the default sink and source")
                }
            },
            Err(err) => {
                log::error!(
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

pub fn get_cards(introspector: &Introspector, chan: flume::Sender<Event>) {
    let cards = Arc::new(Mutex::new(Vec::<Card>::new()));
    let cards_ref = Arc::clone(&cards);

    // used so the thread can signal if it failed to start
    let (tx, rx) = flume::bounded::<bool>(1);

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

            cards_ref.lock().unwrap().push(card_x3);
        }
        ListResult::End => {
            if let Err(err) = tx.send(true) {
                log::error!(
                    "error while sending success for introspector.get_card_info_list: {}",
                    err
                );
            }
        }
        ListResult::Error => {
            log::warn!("could not process introspector.get_card_info_list");
            if let Err(err) = tx.send(false) {
                log::error!(
                    "error while sending failure for introspector.get_card_info_list: {}",
                    err
                );
            }
        }
    });

    thread::spawn(move || {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(success) => match success {
                true => {
                    let data = {
                        let guard = cards.lock().unwrap();
                        guard.clone()
                    };

                    if let Err(err) = chan.send(Event::CardsChanged { cards: data }) {
                        log::error!("[audio] error while sending Message::CardsChanged: {err}");
                    }
                }
                false => {
                    log::warn!("[audio] could not get cards");
                }
            },
            Err(err) => {
                log::error!(
                    "[audio] error while waiting for introspector.get_card_info_list: {}",
                    err
                );
            }
        };
    });
}
