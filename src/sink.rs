use crate::{
    Message, PULSE_MAX_VOLUME,
    audio::{Card, Request, Sink},
};

use chrono::{DateTime, TimeDelta, Utc};
use flume::Sender;

use iced::Task;

use std::sync::{Arc, RwLock};
use std::thread;

use pulse::volume::Volume;

#[derive(Debug, Clone)]
pub enum SinkMessage {
    SelectedSinkChanged(String),
    SinkProfile(String),
    SinkVolume(f32),
    SinkMute,

    SinksChanged(Vec<Sink>),
    DefaultSinkChanged(Option<String>),
}

pub struct SinkWidget {
    pub sinks: Vec<Sink>,
    pub selected_sink: Option<String>,
    /// the pulseaudio name id for the sink
    pub default_sink: Option<String>,
    pub sink_volume: Arc<RwLock<f32>>,
    pub sink_mute: bool,
    pub sink_profiles: Vec<String>,
    pub sink_selected_profile: Option<String>,
    /// the last time either volume slider was set
    pub sink_last_update_time: DateTime<Utc>,
    /// is set to true when a thread is going
    /// to set the volume in the future
    ///
    /// the time until this is set to false
    /// is less than `self.update_frequency`
    pub sink_will_set_volume: bool,
}

impl Default for SinkWidget {
    fn default() -> Self {
        Self {
            sinks: vec![],
            selected_sink: None,
            default_sink: None,
            sink_volume: Arc::new(RwLock::new(35.0)),
            sink_mute: false,
            sink_profiles: vec!["rawr".to_string()],
            sink_selected_profile: None,
            sink_last_update_time: Utc::now(),
            sink_will_set_volume: false,
        }
    }
}

impl SinkWidget {
    fn get_default_sink(&self) -> Option<Sink> {
        if let Some(sink) = &self.selected_sink {
            for s in &self.sinks {
                if sink == &s.description {
                    return Some(s.clone());
                }
            }
        }
        return None;
    }

    pub fn update(
        &mut self,
        message: SinkMessage,
        sender: Sender<Request>,
        update_freq: TimeDelta,
        cards: &Vec<Card>,
    ) -> Task<Message> {
        let command = Task::none();

        match message {
            SinkMessage::SelectedSinkChanged(sink) => {
                self.selected_sink = Some(sink.clone());

                for s in &self.sinks {
                    if sink == s.description {
                        if let Err(err) = sender.send(Request::SetDefaultSink(s.name.clone())) {
                            eprintln!("error while sending Request::SetDefaultSink: {}", err);
                        }
                    }
                }

                let sink: Sink = match self.get_default_sink() {
                    Some(sink) => sink,
                    None => return command,
                };

                if let Some(index) = sink.card_index {
                    for card in cards {
                        if index == card.index {
                            self.sink_profiles = card
                                .profiles
                                .iter()
                                .map(|profile| profile.description.clone())
                                .collect::<Vec<String>>();

                            self.sink_selected_profile = match &card.selected_profile {
                                Some(profile) => Some(profile.description.clone()),
                                None => None,
                            }
                        }
                    }
                }
            }
            SinkMessage::SinkProfile(profile) => {
                self.sink_selected_profile = Some(profile.clone());

                let sink: Sink = match self.get_default_sink() {
                    Some(sink) => sink,
                    None => return command,
                };

                for card in cards {
                    let index = match sink.card_index {
                        Some(i) => i,
                        None => continue,
                    };

                    if index == card.index {
                        for p in &card.profiles {
                            if profile == p.description {
                                if let Err(err) = sender.send(Request::SetCardProfile(
                                    card.name.clone(),
                                    p.name.clone(),
                                )) {
                                    eprintln!(
                                        "error while sending Request::SetCardProfile: {}",
                                        err
                                    );
                                }
                            }
                        }
                    }
                }
            }
            SinkMessage::SinkVolume(volume) => {
                *self.sink_volume.write().unwrap() = volume;

                let t = Utc::now();
                let delta = t - self.sink_last_update_time;
                let is_too_soon = delta < update_freq;
                if is_too_soon && self.sink_will_set_volume {
                    return command;
                }

                let set_volume = move |sink: Sink, sink_volume: f32| {
                    let vol = ((sink_volume / 100.0 * PULSE_MAX_VOLUME as f32).round() as u32)
                        .clamp(0, PULSE_MAX_VOLUME);
                    let mut volume = sink.volume.clone();
                    volume.set(volume.get().len() as u8, Volume(vol));

                    if let Err(err) = sender.send(Request::SetSinkVolume(sink.name.clone(), volume))
                    {
                        eprintln!("error while sending Request::SetSinkVolume: {}", err);
                    }
                };

                let sink: Sink = match self.get_default_sink() {
                    Some(sink) => sink,
                    None => return command,
                };

                if is_too_soon {
                    if !self.sink_will_set_volume {
                        // if this somehow errors, its prob the os and not my code because i don't
                        // get why time would move backwards
                        let wait_time = (update_freq - delta).to_std().expect("HEY! *flusters* i dont know why it did this! its supposed to be in range *sad fox noises*");

                        let volume = Arc::clone(&self.sink_volume);
                        thread::spawn(move || {
                            thread::sleep(wait_time);
                            set_volume(sink, *volume.read().unwrap());
                        });
                        self.sink_will_set_volume = true;
                    }

                    return command;
                }
                self.sink_last_update_time = t;
                self.sink_will_set_volume = false;

                set_volume(sink, *self.sink_volume.read().unwrap());
            }
            SinkMessage::SinkMute => {
                self.sink_mute = !self.sink_mute;
            }
            SinkMessage::SinksChanged(sinks) => {
                self.sinks = sinks;

                if let Some(sink) = &self.default_sink {
                    for s in &self.sinks {
                        if sink == &s.name {
                            self.selected_sink = Some(s.description.clone());

                            let Volume(volume) = s.volume.avg();
                            *self.sink_volume.write().unwrap() =
                                f32::round(volume as f32 / PULSE_MAX_VOLUME as f32 * 100.0);

                            break;
                        }
                    }
                }
            }
            SinkMessage::DefaultSinkChanged(sink) => {
                self.default_sink = sink.clone();

                let sink = match sink {
                    Some(s) => s,
                    None => return command,
                };

                let sink = match self.sinks.iter().find(|s| s.name == sink) {
                    Some(sink) => {
                        self.selected_sink = Some(sink.description.clone());

                        let Volume(volume) = sink.volume.avg();
                        *self.sink_volume.write().unwrap() =
                            f32::round(volume as f32 / PULSE_MAX_VOLUME as f32 * 100.0);

                        sink
                    }
                    None => return command,
                };

                let index = match sink.card_index {
                    Some(i) => i,
                    None => return command,
                };

                let card = match cards.iter().find(|c| c.index == index) {
                    Some(card) => card,
                    None => return command,
                };

                match &card.selected_profile {
                    Some(profile) => {
                        self.sink_selected_profile = Some(profile.description.clone());
                    }
                    None => return command,
                };
            }
        };

        return command;
    }
}
