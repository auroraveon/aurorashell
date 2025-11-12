use super::data::{Card, Request, Sink, Source};
use super::{AudioService, Event, PULSE_MAX_VOLUME, UPDATE_INTERVAL};

use crate::services::{ServiceRequest, ServiceState};

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

use pulse::volume::{ChannelVolumes, Volume};

#[derive(Debug)]
pub struct AudioState {
    /// sinks are the outputs of pulseaudio
    ///
    /// these are like speakers, headphones etc but can also be virtual too
    pub sinks: Vec<Sink>,
    /// the default output of pulseaudio
    pub default_sink: Option<String>,

    /// the profiles associated with the default sink
    pub sink_profiles: Vec<String>,
    /// the profile associated with the default sink
    pub sink_default_profile: Option<String>,

    /// sources are the inputs of pulseaudio
    ///
    /// these are like microphones but can also be virtual too
    pub sources: Vec<Source>,
    /// the default input of pulseaudio
    pub default_source: Option<String>,

    /// the profiles associated with the default source
    pub source_profiles: Vec<String>,
    /// the profile associated with the default source
    pub source_default_profile: Option<String>,

    /// audio cards, sinks and sources map to these
    pub cards: Vec<Card>,
}

impl ServiceState<AudioService> for AudioState {
    fn init() -> Self {
        Self {
            sinks: vec![],
            default_sink: None,
            sink_profiles: vec![],
            sink_default_profile: None,
            sources: vec![],
            default_source: None,
            source_profiles: vec![],
            source_default_profile: None,
            cards: vec![],
        }
    }

    fn update(&mut self, event: Event) -> Vec<Event> {
        let mut _events = match event.clone() {
            Event::SinksChanged { sinks } => {
                self.sinks = sinks;

                vec![]
            }
            Event::DefaultSinkChanged { name } => {
                self.default_sink = name;

                self.update_sink_profile()
            }
            Event::SourcesChanged { sources } => {
                self.sources = sources;

                vec![]
            }
            Event::DefaultSourceChanged { name } => {
                self.default_source = name;

                self.update_source_profile()
            }
            Event::CardsChanged { cards } => {
                self.cards = cards;

                [self.update_sink_profile(), self.update_source_profile()]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<Event>>()
            }
            _ => {
                vec![]
            }
        };

        let mut events = vec![event];
        events.append(&mut _events);
        return events;
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
        channel: &flume::Sender<ServiceRequest<AudioService>>,
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
        channel: &flume::Sender<ServiceRequest<AudioService>>,
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

    fn update_sink_profile(&mut self) -> Vec<Event> {
        let sink: Sink = match self.get_default_sink() {
            Some(sink) => sink,
            None => return vec![],
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

                    return vec![Event::SinkProfileChanged {
                        profile_name: self.sink_default_profile.clone(),
                    }];
                }
            }
        }

        return vec![];
    }

    fn update_source_profile(&mut self) -> Vec<Event> {
        let source: Source = match self.get_default_source() {
            Some(source) => source,
            None => return vec![],
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

                    return vec![Event::SourceProfileChanged {
                        profile_name: self.source_default_profile.clone(),
                    }];
                }
            }
        }

        return vec![];
    }
}

////////////////////////////////////////////////////////////////////////////////

/// state for the audio request thread
#[derive(Debug, Clone)]
pub struct AudioRequestThreadState {
    /// channel for communicating with the service as we use threads here
    /// to slow down rates to the pulseaudio server
    chan: flume::Sender<ServiceRequest<AudioService>>,

    /// the last time we updated certain values (like volume) on the
    /// pulseaudio server for the sink
    sink_last_update_time: Instant,
    /// is true when there is a thread that is scheduled to set the volume
    /// in the future for the sink
    sink_thread_scheduled: bool,
    /// stores the request data for a `Request::SetSinkVolume`
    /// as it will need to be accessed by both the main thread and a
    /// secondary thread that sets the volume 'in the future' to keep
    /// requests to the audio server down
    sink_volume_data: Arc<Mutex<Option<(String, ChannelVolumes)>>>,

    /// the last time we updated certain values (like volume) on the
    /// pulseaudio server for the source
    source_last_update_time: Instant,
    /// is true when there is a thread that is scheduled to set the volume
    /// in the future for the source
    source_thread_scheduled: bool,
    /// stores the request data for a `Request::SetSourceVolume`
    /// as it will need to be accessed by both the main thread and a
    /// secondary thread that sets the volume 'in the future' to keep
    /// requests to the audio server down
    source_volume_data: Arc<Mutex<Option<(String, ChannelVolumes)>>>,
}

impl AudioRequestThreadState {
    pub fn init(chan: flume::Sender<ServiceRequest<AudioService>>) -> Self {
        Self {
            chan,
            sink_last_update_time: Instant::now(),
            sink_thread_scheduled: false,
            sink_volume_data: Arc::new(Mutex::new(None)),
            source_last_update_time: Instant::now(),
            source_thread_scheduled: false,
            source_volume_data: Arc::new(Mutex::new(None)),
        }
    }

    /// returns true if we can set it without invoking a thread or if a thread
    /// is not scheduled
    ///
    /// `name`: name of the sink
    /// `volume`: volume that we set the sink to
    pub fn set_sink_volume(&mut self, name: String, volume: ChannelVolumes) -> bool {
        {
            *self.sink_volume_data.lock().unwrap() = Some((name, volume));
        }

        let now = Instant::now();
        let delta = now - self.sink_last_update_time;

        if delta <= UPDATE_INTERVAL {
            if !self.sink_thread_scheduled {
                // if this somehow errors, its prob the os and not my code because i don't
                // get why time would move backwards >:3
                let wait_time = UPDATE_INTERVAL - delta;

                let chan = self.chan.clone();
                let volume_data = Arc::clone(&self.sink_volume_data);
                thread::spawn(move || {
                    thread::sleep(wait_time);
                    match &*volume_data.lock().unwrap() {
                        Some(data) => {
                            if let Err(err) = chan.send(ServiceRequest::Request {
                                request: Request::SetSinkVolume {
                                    name: data.0.clone(),
                                    volume: data.1,
                                },
                            }) {
                                log::error!(
                                    "[audio] error while sending Request::SetSinkVolume: {}",
                                    err
                                );
                            }
                        }
                        None => {
                            log::warn!(
                                "[audio] could not set sink volume: sink_volume_data is None"
                            );
                        }
                    }
                });
                self.sink_thread_scheduled = true;
            } else {
                // have we waited more than the UPDATE_INTERVAL and is a thread
                // already scheduled to set the volume of the sink in the future?
                return false;
            }
        }

        self.sink_last_update_time = now;
        self.sink_thread_scheduled = false;

        return true;
    }

    /// returns true if we can set it without invoking a thread or if a thread
    /// is not scheduled
    ///
    /// `name`: name of the source
    /// `volume`: volume that we set the source to
    pub fn set_source_volume(&mut self, name: String, volume: ChannelVolumes) -> bool {
        {
            *self.source_volume_data.lock().unwrap() = Some((name, volume));
        }

        let now = Instant::now();
        let delta = now - self.source_last_update_time;

        if delta <= UPDATE_INTERVAL {
            if !self.source_thread_scheduled {
                // if this somehow errors, its prob the os and not my code because i don't
                // get why time would move backwards >:3
                let wait_time = UPDATE_INTERVAL - delta;

                let chan = self.chan.clone();
                let volume_data = Arc::clone(&self.source_volume_data);
                thread::spawn(move || {
                    thread::sleep(wait_time);
                    match &*volume_data.lock().unwrap() {
                        Some(data) => {
                            if let Err(err) = chan.send(ServiceRequest::Request {
                                request: Request::SetSourceVolume {
                                    name: data.0.clone(),
                                    volume: data.1,
                                },
                            }) {
                                log::error!(
                                    "[audio] error while sending Request::SetSourceVolume: {}",
                                    err
                                );
                            }
                        }
                        None => {
                            log::warn!(
                                "[audio] could not set source volume: source_volume_data is None"
                            );
                        }
                    }
                });
                self.source_thread_scheduled = true;
            } else {
                // have we waited more than the UPDATE_INTERVAL and is a thread
                // already scheduled to set the volume of the source in the future?
                return false;
            }
        }

        self.source_last_update_time = now;
        self.source_thread_scheduled = false;

        return true;
    }
}
