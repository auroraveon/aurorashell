use std::ops::{BitOr, BitOrAssign};

#[derive(Debug)]
pub struct LayerSurface {
    /// the id for the surface layer
    ///
    /// a unique one is fetched from the wasm host
    pub id: Id,
    pub layer: Layer,
    pub anchor: Anchor,
    pub size: Option<(Option<u32>, Option<u32>)>,
    pub margin: Margin,
    pub limits: Limits,
    pub exclusive_zone: i32,
    pub keyboard_interactivity: KeyboardInteractivity,
    pub pointer_interactivity: bool,
}

impl Default for LayerSurface {
    fn default() -> Self {
        Self {
            id: Id::unique(IdType::LayerSurface),
            layer: Layer::Top,
            anchor: Anchor::none(),
            size: Default::default(),
            margin: Default::default(),
            limits: Default::default(),
            exclusive_zone: Default::default(),
            keyboard_interactivity: Default::default(),
            pointer_interactivity: true,
        }
    }
}

/// represents the raw data for a `LayerSurface` so the wasm host can safely
/// read the data
#[repr(C)]
#[derive(Debug)]
pub struct LayerSurfaceRaw {
    pub id: u32,
    /// `Layer` gets converted to a u8
    pub layer: u8,
    /// `Anchor`'s internal value
    pub anchor: u8,
    /// 1st bit - size: 0 = None, 1 = Some(Option<u32>, Option<u32>)
    /// 2nd bit - x dir: 0 = None, 1 = Some(u32)
    /// 3rd bit - y dir: 0 = None, 1 = Some(u32)
    pub size_flags: u8,
    pub size_x: u32,
    pub size_y: u32,
    pub margin_ptr: u32,
    pub limits_ptr: u32,
    pub exclusive_zone: i32,
    /// `KeyboardInteractivity` gets converted to a u8
    pub keyboard_interactivity: u8,
    /// boolean for pointer interactivity is converted to a u8 to be safe
    /// to transport between wasm host and guest
    pub pointer_interactivity: u8,
}

#[repr(u32)]
#[derive(Debug)]
pub enum IdType {
    None,
    LayerSurface,
}

unsafe extern "C" {
    /// host function to get a unique id from the wasm runtime
    fn get_unique_id(id_type: u32) -> u32;
}

/// represents an id that is determined by the wasm host
#[derive(Debug, Default, Clone, Copy)]
pub struct Id(u32);

impl Id {
    pub fn get_id(&self) -> u32 {
        self.0
    }

    /// gets a unique id from the wasm host
    pub fn unique(id_type: IdType) -> Id {
        unsafe { Id(get_unique_id(id_type as u32)) }
    }
}

#[repr(u8)]
#[derive(Debug, Clone)]
pub enum Layer {
    Background = 0,
    Bottom = 1,
    Top = 2,
    Overlay = 3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Anchor(pub u8);

impl Anchor {
    pub const TOP: Self = Self(0b0001);
    pub const BOTTOM: Self = Self(0b0010);
    pub const LEFT: Self = Self(0b0100);
    pub const RIGHT: Self = Self(0b1000);
}

impl Anchor {
    pub fn none() -> Anchor {
        Anchor(0b0000)
    }

    pub fn all() -> Anchor {
        Anchor(0b1111)
    }
}

impl From<u8> for Anchor {
    fn from(value: u8) -> Self {
        Anchor(value)
    }
}

impl BitOr for Anchor {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for Anchor {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

#[repr(C)]
#[derive(Debug, Default)]
pub struct Margin {
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
    pub left: i32,
}

#[repr(C)]
#[derive(Debug)]
pub struct Limits {
    pub min_width: f32,
    pub max_width: f32,
    pub min_height: f32,
    pub max_height: f32,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            min_width: 1.0,
            max_width: 1920.0,
            min_height: 1.0,
            max_height: 1080.023,
        }
    }
}

#[repr(u8)]
#[derive(Debug, Clone)]
pub enum KeyboardInteractivity {
    None,
    Exclusive,
    OnDemand,
}

impl Default for KeyboardInteractivity {
    fn default() -> Self {
        Self::None
    }
}
