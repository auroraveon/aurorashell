use crate::{
    app::{AppMessage, ServiceMessage, UiEvent},
    services::{
        ActiveService, ServiceEvent,
        audio::{self, AudioService, AudioState, PULSE_MAX_VOLUME, Sink},
    },
    theme::{self, Base16Color},
};

use iced::alignment::Vertical;
use iced::widget::{button, column, container, pick_list, row, slider, text};
use iced::window::Id;
use iced::{Background, Color, Element, Font, Task, Theme};

use std::sync::{Arc, RwLock};

use pulse::volume::Volume;

#[derive(Debug)]
pub struct SinkWidget {
    pub ui_selected_sink: Option<String>,
    pub ui_sink_selected_profile: Option<String>,
    pub ui_sink_volume: Arc<RwLock<f32>>,
    pub ui_sink_mute: bool,

    pub sinks: Vec<Sink>,
    /// the pulseaudio name id for the sink
    pub default_sink: Option<String>,
    pub sink_profiles: Vec<String>,
}

impl Default for SinkWidget {
    fn default() -> Self {
        Self {
            sinks: vec![],
            ui_selected_sink: None,
            default_sink: None,
            ui_sink_volume: Arc::new(RwLock::new(35.0)),
            ui_sink_mute: false,
            sink_profiles: vec!["rawr".to_string()],
            ui_sink_selected_profile: None,
        }
    }
}

impl SinkWidget {
    fn get_default_sink(&self) -> Option<Sink> {
        if let Some(sink) = &self.default_sink {
            for s in &self.sinks {
                if sink == &s.name {
                    return Some(s.clone());
                }
            }
        }
        return None;
    }

    pub fn update(
        &mut self,
        message: &AppMessage,
        audio: &mut AudioState,
    ) -> Option<Task<AppMessage>> {
        let command: Option<Task<AppMessage>> = None;

        match message {
            AppMessage::Ui(event) => match event {
                UiEvent::SinkChanged(sink) => {
                    self.ui_selected_sink = Some(sink.clone());

                    for s in &self.sinks {
                        if *sink == s.description {
                            if let Err(err) = AudioService::request(
                                audio,
                                audio::Request::SetDefaultSink {
                                    name: s.name.clone(),
                                },
                            ) {
                                eprintln!("error while sending Request::SetDefaultSink: {}", err);
                            }
                        }
                    }

                    let sink: Sink = match self.get_default_sink() {
                        Some(sink) => sink,
                        None => return command,
                    };

                    if let Some(index) = sink.card_index {
                        for card in &audio.cards {
                            if index == card.index {
                                self.sink_profiles = card
                                    .profiles
                                    .iter()
                                    .map(|profile| profile.description.clone())
                                    .collect::<Vec<String>>();

                                self.ui_sink_selected_profile = match &card.selected_profile {
                                    Some(profile) => Some(profile.description.clone()),
                                    None => None,
                                }
                            }
                        }
                    }
                }
                UiEvent::SinkProfile(profile) => {
                    self.ui_sink_selected_profile = Some(profile.clone());

                    let sink: Sink = match self.get_default_sink() {
                        Some(sink) => sink,
                        None => return command,
                    };

                    for card in audio.cards.clone() {
                        let index = match sink.card_index {
                            Some(i) => i,
                            None => continue,
                        };

                        if index == card.index {
                            for p in &card.profiles {
                                if *profile == p.description {
                                    if let Err(err) = AudioService::request(
                                        audio,
                                        audio::Request::SetCardProfile {
                                            card_name: card.name.clone(),
                                            profile_name: p.name.clone(),
                                        },
                                    ) {
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
                UiEvent::SinkVolume(volume) => {
                    *self.ui_sink_volume.write().unwrap() = *volume;

                    let sink = match audio.get_default_sink() {
                        Some(sink) => sink,
                        None => return command,
                    };

                    let channel_volume = AudioState::set_channel_volume(sink.volume, *volume);

                    if let Err(err) = AudioService::request(
                        audio,
                        audio::Request::SetSinkVolume {
                            name: sink.name.clone(),
                            volume: channel_volume,
                        },
                    ) {
                        eprintln!("error while sending Request::SetSinkVolume: {}", err);
                    }
                }
                UiEvent::SinkMute => {
                    self.ui_sink_mute = !self.ui_sink_mute;

                    let sink: Sink = match self.get_default_sink() {
                        Some(sink) => sink,
                        None => return command,
                    };

                    if let Err(err) = AudioService::request(
                        audio,
                        audio::Request::SetSinkMute {
                            name: sink.name.clone(),
                            state: self.ui_sink_mute,
                        },
                    ) {
                        eprintln!("error while sending Request::SetSinkMute: {}", err);
                    }
                }
                _ => {}
            },
            AppMessage::Service(ServiceMessage::Audio(event)) => match event {
                ServiceEvent::Init { state: _ } => (),
                ServiceEvent::Update { event } => match event {
                    audio::Event::SinksChanged { sinks } => {
                        self.sinks = sinks.clone();

                        let sink: Sink = match self.get_default_sink() {
                            Some(sink) => sink,
                            None => return command,
                        };

                        self.ui_selected_sink = Some(sink.description.clone());

                        if let Some(index) = sink.card_index {
                            for card in &audio.cards {
                                if index != card.index {
                                    continue;
                                }

                                if let Some(profile) = &card.selected_profile {
                                    self.ui_sink_selected_profile =
                                        Some(profile.description.clone());
                                }
                            }
                        }

                        let Volume(volume) = sink.volume.avg();
                        *self.ui_sink_volume.write().unwrap() =
                            f32::round(volume as f32 / PULSE_MAX_VOLUME as f32 * 100.0);

                        self.ui_sink_mute = sink.mute;
                    }
                    audio::Event::DefaultSinkChanged { name } => {
                        self.default_sink = name.clone();

                        let sink = match name {
                            Some(s) => s,
                            None => return command,
                        };

                        let sink = match self.sinks.iter().find(|s| s.name == *sink) {
                            Some(sink) => {
                                self.ui_selected_sink = Some(sink.description.clone());

                                let Volume(volume) = sink.volume.avg();
                                *self.ui_sink_volume.write().unwrap() =
                                    f32::round(volume as f32 / PULSE_MAX_VOLUME as f32 * 100.0);

                                sink
                            }
                            None => return command,
                        };

                        let index = match sink.card_index {
                            Some(i) => i,
                            None => return command,
                        };

                        let card = match audio.cards.iter().find(|c| c.index == index) {
                            Some(card) => card,
                            None => return command,
                        };

                        self.ui_sink_selected_profile = match &card.selected_profile {
                            Some(profile) => Some(profile.description.clone()),
                            None => return command,
                        };
                    }
                    _ => {}
                },
            },
            _ => {}
        };

        return command;
    }

    pub fn view<'a>(&self, _id: Id, theme: &'a Base16Color, font: Font) -> Element<'a, AppMessage> {
        let sinks = self
            .sinks
            .iter()
            .map(|sink| sink.description.clone())
            .collect::<Vec<String>>();

        column![
            text("Output")
                .style(theme::text_style(theme))
                .font(font)
                .size(11),
            pick_list(sinks.clone(), self.ui_selected_sink.clone(), |sink| {
                AppMessage::Ui(UiEvent::SinkChanged(sink))
            })
            .style(theme::pick_list_style(theme))
            .menu_style(theme::pick_list_menu_style(theme))
            .font(font)
            .text_size(11)
            .text_wrap(text::Wrapping::WordOrGlyph),
            pick_list(
                self.sink_profiles.clone(),
                self.ui_sink_selected_profile.clone(),
                |profile| { AppMessage::Ui(UiEvent::SinkProfile(profile)) }
            )
            .style(theme::pick_list_style(theme))
            .menu_style(theme::pick_list_menu_style(theme))
            .font(font)
            .text_size(11),
            row![
                button(
                    text(match self.ui_sink_mute {
                        true => "",
                        false => "",
                    })
                    .font(font)
                    .size(11)
                )
                .on_press(AppMessage::Ui(UiEvent::SinkMute))
                .style(theme::volume_button_style(theme)),
                text(format!("{}%", *self.ui_sink_volume.read().unwrap()))
                    .style(theme::text_style(theme))
                    .font(font)
                    .size(11),
                container(
                    slider(
                        0.0..=100.0,
                        *self.ui_sink_volume.read().unwrap(),
                        |volume| { AppMessage::Ui(UiEvent::SinkVolume(volume)) }
                    )
                    .style(theme::slider_style(theme))
                    .step(5.0)
                    .shift_step(1.0)
                )
                .height(6)
                .style(|_: &Theme| container::Style {
                    background: Some(Background::Color(theme.color01)),
                    border: iced::Border {
                        color: Color::TRANSPARENT,
                        width: 0.0,
                        radius: iced::Radius::new(128),
                    },
                    ..container::Style::default()
                }),
            ]
            .spacing(8)
            .align_y(Vertical::Center),
        ]
        .spacing(8)
        .into()
    }
}
