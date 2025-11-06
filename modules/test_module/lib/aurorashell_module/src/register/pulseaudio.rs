use std::ops::{BitOr, BitOrAssign};

use super::{IntoRegister, RegisterTrait};

#[derive(Debug)]
pub struct PulseAudio(u8);

impl PulseAudio {
    /// subscribes to the list of sinks changing
    pub const SINKS_CHANGED: Self = Self(0b_0000_0001);
    /// subscribes to default sink changes
    pub const DEFAULT_SINK_CHANGED: Self = Self(0b_0000_0010);
    /// subscribes to the list of sources changing
    pub const SOURCES_CHANGED: Self = Self(0b_0000_0100);
    /// subscribes to default sink changes
    pub const DEFAULT_SOURCE_CHANGED: Self = Self(0b_0000_1000);
    /// subscribes to the list of cards changing
    pub const CARDS_CHANGED: Self = Self(0b_0001_0000);
    /// subscribes to default sink's current profile changing
    pub const SINK_PROFILE_CHANGED: Self = Self(0b_0010_0000);
    /// subscribes to default sink's current profile changing
    pub const SOURCE_PROFILE_CHANGED: Self = Self(0b_0100_0000);
}

impl PulseAudio {
    pub fn none() -> Self {
        Self(0)
    }

    pub fn all() -> Self {
        Self(0b0111_1111)
    }
}

impl Default for PulseAudio {
    fn default() -> Self {
        Self::none()
    }
}

impl BitOr for PulseAudio {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for PulseAudio {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl RegisterTrait for PulseAudio {
    fn id(&self) -> u16 {
        PulseAudio::const_id()
    }

    fn allow_duplicates(&self) -> bool {
        PulseAudio::const_allow_duplicates()
    }

    fn registers(&self) -> u32 {
        self.0 as u32
    }

    fn serialize(&self) -> Option<Vec<u8>> {
        return None;
    }
}

impl IntoRegister for PulseAudio {}

impl PulseAudio {
    pub const fn const_id() -> u16 {
        0x00_01
    }

    pub const fn const_allow_duplicates() -> bool {
        false
    }
}
