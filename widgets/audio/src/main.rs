mod audio;
mod theme;

use audio::{Card, Request, Sink, Source};

use chrono::{DateTime, TimeDelta, Utc};

use iced::advanced::layout::Limits;
use iced::daemon::Appearance;
use iced::futures::{SinkExt, Stream};
use iced::platform_specific::runtime::wayland::layer_surface::SctkLayerSurfaceSettings;
use iced::platform_specific::shell::commands::layer_surface::{
    Anchor, KeyboardInteractivity, Layer, get_layer_surface,
};
use iced::runtime::platform_specific::wayland::layer_surface::IcedMargin;
use iced::widget::{button, column, pick_list, row, slider, text};
use iced::window::Id;
use iced::{stream, Color, Element, Subscription, Theme};


use pulse::volume::Volume;

use std::sync::{Arc, RwLock};
use std::thread;

const PULSE_MAX_VOLUME: u32 = 65536;

pub fn main() -> iced::Result {
    // run app!!! :3
    iced::daemon(App::title, App::update, App::view)
        .subscription(App::subscription)
        .theme(App::theme)
        .style(App::style)
        .run_with(App::new)
}

struct App {
    font: Font,
    sender: Option<flume::Sender<Request>>,

    update_frequency: TimeDelta,

    sinks: Vec<Sink>,
    selected_sink: Option<String>,
    /// the pulseaudio name id for the sink
    default_sink: Option<String>,
    sink_volume: Arc<RwLock<f32>>,
    sink_mute: bool,
    sink_profiles: Vec<String>,
    sink_selected_profile: Option<String>,
    /// the last time either volume slider was set
    sink_last_update_time: DateTime<Utc>,
    /// is set to true when a thread is going
    /// to set the volume in the future
    ///
    /// the time until this is set to false
    /// is less than `self.update_frequency`
    sink_will_set_volume: bool,

    sources: Vec<Source>,
    selected_source: Option<String>,
    /// the pulseaudio name id for the source
    default_source: Option<String>,
    source_volume: Arc<RwLock<f32>>,
    /// the last time either volume slider was set
    source_last_update_time: DateTime<Utc>,
    /// is set to true when a thread is going
    /// to set the volume in the future
    ///
    /// the time until this is set to false
    /// is less than `self.update_frequency`
    source_will_set_volume: bool,

    cards: Vec<Card>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            font: Font::with_name("DepartureMono Nerd Font"),
            sender: None,
            update_frequency: TimeDelta::milliseconds(100),
            // default sink stuff
            sinks: vec![],
            selected_sink: None,
            default_sink: None,
            sink_volume: Arc::new(RwLock::new(35.0)),
            sink_mute: false,
            sink_profiles: vec!["rawr".to_string()],
            sink_selected_profile: None,
            sink_last_update_time: Utc::now(),
            sink_will_set_volume: false,
            // default source stuff
            sources: vec![],
            selected_source: None,
            default_source: None,
            source_volume: Arc::new(RwLock::new(55.0)),
            source_last_update_time: Utc::now(),
            source_will_set_volume: false,
            // cards
            cards: vec![],
        }
    }
}

#[derive(Debug, Clone)]
enum Message {
    /// event for when the sender is created
    /// by the subscription worker
    ChannelCreated(flume::Sender<Request>),

    SelectedSinkChanged(String),
    SinkVolume(f32),
    SinkMute,
    SinkProfile(String),

    SelectedSourceChanged(String),
    SourceVolume(f32),

    // --- pulseaudio events ---
    SinksChanged(Vec<Sink>),
    DefaultSinkChanged(Option<String>),
    SourcesChanged(Vec<Source>),
    DefaultSourceChanged(Option<String>),
    CardsChanged(Vec<Card>),
}

impl App {
    fn new() -> (App, Command<Message>) {
        let mut initial_surface = SctkLayerSurfaceSettings::default();
        initial_surface.layer = Layer::Top;
        initial_surface.anchor = Anchor::TOP | Anchor::RIGHT;
        initial_surface.margin = IcedMargin {
            top: 8,
            right: 20,
            bottom: 0,
            left: 0,
        };
        initial_surface.size_limits = Limits::NONE;
        initial_surface.size = Some((Some(320), Some(230)));

        initial_surface.keyboard_interactivity = KeyboardInteractivity::OnDemand;
        
        (Self::default(), get_layer_surface(initial_surface))
    }

    fn title(&self, _id: Id) -> String {
        String::from("Aurora Audio Widget")
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        let command = Command::none();

        match message {
            Message::ChannelCreated(sender) => {
                self.sender = Some(sender);
            }
            Message::SelectedSinkChanged(sink) => {
                self.selected_sink = Some(sink.clone());

                if let Some(sender) = self.sender.clone() {
                    for s in &self.sinks {
                        if sink == s.description {
                            if let Err(err) = sender.send(Request::SetDefaultSink(s.name.clone())) {
                                eprintln!("error while sending Request::SetDefaultSink: {}", err);
                            }
                        }
                    }
                }

                let sink: Sink = {
                    let mut sink: Option<Sink> = None;
                    if let Some(_sink) = &self.selected_sink {
                        for s in &self.sinks {
                            if _sink == &s.description {
                                sink = Some(s.clone());
                                break;
                            }
                        }
                    }
                    match sink {
                        Some(sink) => sink,
                        None => return command,
                    }
                };

                if let Some(index) = sink.card_index {
                    for card in &self.cards {
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
            Message::SinkVolume(volume) => {
                *self.sink_volume.write().unwrap() = volume;

                let t = Utc::now();
                let delta = t - self.sink_last_update_time;
                let is_too_soon = delta < self.update_frequency;
                if is_too_soon && self.sink_will_set_volume {
                    return command;
                }

                let sender = match &self.sender {
                    Some(v) => v.clone(),
                    None => {
                        eprintln!("no sender available");
                        return command;
                    }
                };

                let set_volume = move |sink: Option<Sink>, sink_volume: f32| {
                    if let Some(sink) = sink {
                        let vol = ((sink_volume / 100.0 * PULSE_MAX_VOLUME as f32).round() as u32)
                            .clamp(0, PULSE_MAX_VOLUME);
                        let mut volume = sink.volume.clone();
                        volume.set(volume.get().len() as u8, Volume(vol));

                        if let Err(err) =
                            sender.send(Request::SetSinkVolume(sink.name.clone(), volume))
                        {
                            eprintln!("error while sending Request::SetSinkVolume: {}", err);
                        }
                    }
                };

                let mut sink: Option<Sink> = None;
                if let Some(_sink) = &self.default_sink {
                    for s in &self.sinks {
                        if _sink == &s.name {
                            sink = Some(s.clone());
                        }
                    }
                }

                if is_too_soon {
                    if !self.sink_will_set_volume {
                        let wait_time = (self.update_frequency - delta).to_std().expect("HEY! *flusters* i dont know why it did this! its supposed to be in range *sad fox noises*");

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
            Message::SinkMute => {
                self.sink_mute = !self.sink_mute;
            }
            Message::SinkProfile(profile) => {
                self.sink_selected_profile = Some(profile.clone());

                let sender = match &self.sender {
                    Some(s) => s.clone(),
                    None => return command,
                };

                let sink: Sink = {
                    let mut sink: Option<Sink> = None;
                    if let Some(_sink) = &self.default_sink {
                        for s in &self.sinks {
                            if _sink == &s.name {
                                sink = Some(s.clone());
                                break;
                            }
                        }
                    }
                    match sink {
                        Some(sink) => sink,
                        None => return command,
                    }
                };

                for card in &self.cards {
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
            Message::SelectedSourceChanged(source) => {
                self.selected_source = Some(source.clone());

                if let Some(sender) = self.sender.clone() {
                    for s in &self.sources {
                        if source == s.description {
                            if let Err(err) = sender.send(Request::SetDefaultSource(s.name.clone()))
                            {
                                eprintln!("error while sending Request::SetDefaultSource: {}", err);
                            }
                        }
                    }
                }
            }
            Message::SourceVolume(volume) => {
                *self.source_volume.write().unwrap() = volume;

                let t = Utc::now();
                let delta = t - self.source_last_update_time;
                let is_too_soon = delta < self.update_frequency;
                if is_too_soon && self.source_will_set_volume {
                    return command;
                }

                let sender = match &self.sender {
                    Some(v) => v.clone(),
                    None => {
                        eprintln!("no sender available");
                        return command;
                    }
                };

                let set_volume = move |source: Option<Source>, source_volume: f32| {
                    if let Some(source) = source {
                        let vol = ((source_volume / 100.0 * PULSE_MAX_VOLUME as f32).round()
                            as u32)
                            .clamp(0, PULSE_MAX_VOLUME);
                        let mut volume = source.volume.clone();
                        volume.set(volume.get().len() as u8, Volume(vol));

                        if let Err(err) =
                            sender.send(Request::SetSourceVolume(source.name.clone(), volume))
                        {
                            eprintln!("error while sending Request::SetSourceVolume: {}", err);
                        }
                    }
                };

                let mut source: Option<Source> = None;
                if let Some(_source) = &self.default_source {
                    for s in &self.sources {
                        if _source == &s.name {
                            source = Some(s.clone());
                        }
                    }
                }

                if is_too_soon {
                    if !self.source_will_set_volume {
                        let wait_time = (self.update_frequency - delta).to_std().expect("HEY! *flusters* i dont know why it did this! its supposed to be in range *sad fox noises*");

                        let volume = Arc::clone(&self.source_volume);
                        thread::spawn(move || {
                            thread::sleep(wait_time);
                            set_volume(source, *volume.read().unwrap());
                        });
                        self.source_will_set_volume = true;
                    }

                    return command;
                }
                self.source_last_update_time = t;
                self.source_will_set_volume = false;

                set_volume(source, *self.source_volume.read().unwrap());
            }
            Message::SinksChanged(sinks) => {
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
            Message::DefaultSinkChanged(sink) => {
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

                let card = match self.cards.iter().find(|c| c.index == index) {
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
            Message::SourcesChanged(sources) => {
                self.sources = sources;

                if let Some(source) = &self.default_source {
                    for s in &self.sources {
                        if source == &s.name {
                            self.selected_source = Some(s.description.clone());

                            let Volume(volume) = s.volume.avg();
                            *self.source_volume.write().unwrap() =
                                f32::round(volume as f32 / PULSE_MAX_VOLUME as f32 * 100.0);

                            break;
                        }
                    }
                }
            }
            Message::DefaultSourceChanged(source) => {
                self.default_source = source.clone();

                if let Some(source) = source {
                    for s in &self.sources {
                        if source == s.name {
                            self.selected_source = Some(s.description.clone());

                            let Volume(volume) = s.volume.avg();
                            *self.source_volume.write().unwrap() =
                                f32::round(volume as f32 / PULSE_MAX_VOLUME as f32 * 100.0);

                            break;
                        }
                    }
                }
            }
            Message::CardsChanged(cards) => {
                self.cards = cards.clone();

                println!("AM FOX :D");

                let sink: Sink = {
                    let mut sink: Option<Sink> = None;
                    if let Some(_sink) = &self.default_sink {
                        for s in &self.sinks {
                            if _sink == &s.name {
                                sink = Some(s.clone());
                                break;
                            }
                        }
                    }
                    match sink {
                        Some(sink) => sink,
                        None => return command,
                    }
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
        }

        return command;
    }

    fn view(&self, _: Id) -> Element<Message> {
        let sinks = self
            .sinks
            .iter()
            .map(|sink| sink.description.clone())
            .collect::<Vec<String>>();

        let sources = self
            .sources
            .iter()
            .map(|source| source.description.clone())
            .collect::<Vec<String>>();

        let sink_ui = column![
            text("Output").font(self.font).size(11),
            pick_list(sinks.clone(), self.selected_sink.clone(), |sink| {
                Message::SelectedSinkChanged(sink)
            })
            .font(self.font)
            .text_size(11)
            .text_wrap(text::Wrapping::WordOrGlyph),
            pick_list(
                self.sink_profiles.clone(),
                self.sink_selected_profile.clone(),
                |profile| { Message::SinkProfile(profile) }
            )
            .font(self.font)
            .text_size(11),
            row![
                button(match self.sink_mute {
                    true => "",
                    false => "",
                })
                .on_press(Message::SinkMute),
                text(format!("{}%", *self.sink_volume.read().unwrap()))
                    .font(self.font)
                    .size(11),
                slider(0.0..=100.0, *self.sink_volume.read().unwrap(), |volume| {
                    Message::SinkVolume(volume)
                })
                .step(5.0)
                .shift_step(1.0),
            ]
            .spacing(8),
        ]
        .spacing(8);

        let source_ui = column![
            text("Input").font(self.font).size(11),
            pick_list(sources.clone(), self.selected_source.clone(), |source| {
                Message::SelectedSourceChanged(source)
            })
            .font(self.font)
            .text_size(11),
            row![
                text(format!("{}%", *self.source_volume.read().unwrap()))
                    .font(self.font)
                    .size(11),
                slider(0.0..=100.0, *self.source_volume.read().unwrap(), |volume| {
                    Message::SourceVolume(volume)
                })
                .step(5.0)
                .shift_step(1.0),
            ]
            .spacing(8),
        ]
        .spacing(8);

        column![sink_ui, source_ui].padding(8).spacing(16).into()
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::run(App::worker)
    }

    fn theme(&self, _id: Id) -> iced::Theme {
        Theme::Ferra
    }

    fn style(&self, theme: &Theme) -> Appearance {
        Appearance {
            background_color: Color::TRANSPARENT,
            text_color: theme.palette().text,
            icon_color: theme.palette().text,
        }
    }
}

impl App {
    fn worker() -> impl Stream<Item = Message> {
        stream::channel(100, async move |mut chan| {
            // if this has an error the application probably won't close
            // but instead won't interact with pulseaudio
            let (_handle, audio_tx, audio_rx) = match audio::run() {
                Ok(res) => res,
                Err(err) => {
                    eprintln!("error while trying to run pulseaudio mainloop: {err}");
                    return;
                }
            };

            let (tx, rx) = flume::bounded::<Request>(100);

            if let Err(err) = chan.send(Message::ChannelCreated(tx.clone())).await {
                eprintln!("error while sending tx to app: {}", err);
            }

            loop {
                tokio::select! {
                    result = audio_rx.recv_async() => {
                        //println!("x3");
                        let msg = match result {
                            Ok(res) => res,
                            Err(err) => {
                                eprintln!("error while receiving message in subscription worker: {err}");
                                return;
                            },
                        };

                        match msg {
                            audio::Message::SinksChanged(sinks) => {
                                if let Err(err) = chan.send(Message::SinksChanged(sinks)).await {
                                    eprintln!("error while sending Message:SinksChanged: {}", err);
                                }
                            },
                            audio::Message::DefaultSinkChanged(sink) => {
                                if let Err(err) = chan.send(Message::DefaultSinkChanged(sink)).await {
                                    eprintln!("error while sending Message:DefaultSinkChanged: {}", err);
                                }
                            },
                            audio::Message::SourcesChanged(sources) => {
                                if let Err(err) = chan.send(Message::SourcesChanged(sources)).await {
                                    eprintln!("error while sending Message:SourcesChanged: {}", err);
                                }
                            },
                            audio::Message::DefaultSourceChanged(source) => {
                                if let Err(err) = chan.send(Message::DefaultSourceChanged(source)).await {
                                    eprintln!("error while sending Message:DefaultSourceChanged: {}", err);
                                }
                            },
                            audio::Message::CardsChanged(cards) => {
                                if let Err(err) = chan.send(Message::CardsChanged(cards)).await {
                                    eprintln!("error while sending Message:CardsChanged: {}", err);
                                }
                            }
                        };
                    },
                    result = rx.recv_async() => {
                        let msg = match result {
                            Ok(res) => res,
                            Err(err) => {
                                eprintln!("error while receiving request in subscription worker: {err}");
                                return;
                            },
                        };

                        match msg {
                            Request::SetDefaultSink(sink) => {
                                if let Err(err) = audio_tx.send(audio::Request::SetDefaultSink(sink)) {
                                    eprintln!("error while sending Request::SetDefaultSink: {}", err)
                                }
                            },
                            Request::SetSinkVolume(sink, volume) => {
                                //println!("*wags tail, ears perk up* :3");
                                if let Err(err) = audio_tx.send(audio::Request::SetSinkVolume(sink, volume)) {
                                    eprintln!("error while sending Request::SetSinkVolume: {}", err)
                                }
                            },
                            Request::SetDefaultSource(source) => {
                                if let Err(err) = audio_tx.send(audio::Request::SetDefaultSource(source)) {
                                    eprintln!("error while sending Request::SetDefaultSource: {}", err)
                                }
                            },
                            Request::SetSourceVolume(sink, volume) => {
                                if let Err(err) = audio_tx.send(audio::Request::SetSourceVolume(sink, volume)) {
                                    eprintln!("error while sending Request::SetSourceVolume: {}", err)
                                }
                            },
                            Request::SetCardProfile(card, profile) => {
                                if let Err(err) = audio_tx.send(audio::Request::SetCardProfile(card, profile)) {
                                    eprintln!("error while sending Request::SetCardProfile: {}", err)
                                }
                            },
                        };
                    },
                };
            }
        })
    }
}
