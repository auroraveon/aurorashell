use aurorashell_module::{
    Element, MessageError, column,
    macros::{create_module, registers},
    register::{Interval, PulseAudio},
    row,
    setup::SetupData,
    surface::{Anchor, Id, IdType, Layer, LayerSurface, Margin},
    widget::{Button, Slider, Text},
};

create_module! { // //
    Module,         //
    Module::new,    //
    Module::update, //
    Module::view,   //
    Message,        //
} // -------------- //

#[derive(Debug, Default)]
pub struct Module {
    test_surface_id: Id,
    test_surface_id_2: Id,

    button_state: bool,
    slider_value: f64,
    slider_value2: f64,
}

#[derive(Debug)]
pub enum Message {
    ButtonClicked,
    SliderValue(f64),
    SliderValue2(f64),
}

impl From<Message> for u32 {
    fn from(value: Message) -> Self {
        match value {
            Message::ButtonClicked => 1,
            Message::SliderValue(_) => 2,
            Message::SliderValue2(_) => 3,
        }
    }
}

impl Message {
    fn try_from(id: u32, data_ptr: u32) -> Result<Self, MessageError> {
        Ok(match id {
            1 => Message::ButtonClicked,
            2 => {
                let data = unsafe { Box::from_raw(data_ptr as *mut f64) };
                Message::SliderValue(*data)
            }
            3 => {
                let data = unsafe { Box::from_raw(data_ptr as *mut f64) };
                Message::SliderValue2(*data)
            }
            _ => return Err(MessageError(format!("{} is not a valid message id", id))),
        })
    }
}

impl Module {
    fn new() -> (Module, SetupData) {
        let id = Id::unique(IdType::LayerSurface);
        let id_2 = Id::unique(IdType::LayerSurface);

        let awrawrawr = Interval::from_millis(3000);

        (
            Module {
                test_surface_id: id,
                test_surface_id_2: id_2,
                button_state: false,
                slider_value: 50.0,
                slider_value2: 70.0,
            },
            SetupData {
                module_name: "bar_clock_module".to_string(),
                layer_surfaces: vec![
                    LayerSurface {
                        id,
                        layer: Layer::Top,
                        anchor: Anchor::TOP | Anchor::LEFT,
                        size: Some((Some(320), Some(240))),
                        margin: Margin {
                            top: 0,
                            right: 0,
                            bottom: 12,
                            left: 20,
                        },
                        ..Default::default()
                    },
                    LayerSurface {
                        id: id_2,
                        layer: Layer::Top,
                        anchor: Anchor::BOTTOM | Anchor::RIGHT,
                        size: Some((Some(320), Some(240))),
                        margin: Margin {
                            top: 0,
                            right: 20,
                            bottom: 12,
                            left: 0,
                        },
                        ..Default::default()
                    },
                ],
                registers: registers![
                    Interval::from_millis(1000),
                    Interval::from_millis(2000),
                    PulseAudio::SINKS | PulseAudio::SOURCES,
                    Interval: awrawrawr,
                ],
            },
        )
    }

    fn update(&mut self, message: Message) -> Option<Message> {
        match message {
            Message::ButtonClicked => {
                self.button_state = !self.button_state;
                None
            }
            Message::SliderValue(value) => {
                self.slider_value = value;
                None
            }
            Message::SliderValue2(value) => {
                self.slider_value2 = value;
                None
            }
        }
    }

    fn view(&self, id: u32) -> Element<Message> {
        if id == self.test_surface_id.get_id() {
            let button_text = match self.button_state {
                false => "false",
                true => "true",
            };

            column![
                Text::new("yay"),
                Text::new("am so fox >:3"),
                Text::new("mlem is so gay!! <3"),
                Text::new("*huggggggg* :3"),
                row![
                    Text::new("*cuddles ava* :33333"),
                    Text::new("*kisses ava* <3333 -w-"),
                ],
                Button::new(Element::new(Text::new(button_text)))
                    .on_press(Box::new(|| { Message::ButtonClicked.into() })),
                Text::new(format!("slider value = {}", self.slider_value)),
                Slider::new(
                    0.0..=100.0,
                    self.slider_value,
                    Box::new(|value| { (Message::SliderValue(value).into(), value) })
                ),
            ]
            .into()
        } else if id == self.test_surface_id_2.get_id() {
            column![
                Slider::new(
                    0.0..=100.0,
                    self.slider_value,
                    Box::new(|value| { (Message::SliderValue(value).into(), value) })
                ),
                Slider::new(
                    0.0..=100.0,
                    self.slider_value2,
                    Box::new(|value| { (Message::SliderValue2(value).into(), value) })
                ),
            ]
            .into()
        } else {
            Text::new("cries all over it").into()
        }
    }
}
