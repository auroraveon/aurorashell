//! this module contains types that are shared between the runtime and
//! a runtime module

use std::ops::{BitAnd, BitOr, BitOrAssign};

#[derive(Debug, Clone)]
pub enum Register {
    Interval { milliseconds: u64, offset: u32 },
    PulseAudio { pulseaudio: AudioRegisterData },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioRegisterData(pub u8);

impl AudioRegisterData {
    /// subscribes to the list of cards changing
    pub const CARDS_CHANGED: Self = Self(0b_0001_0000);
    /// subscribes to default sink changes
    pub const DEFAULT_SINK_CHANGED: Self = Self(0b_0000_0010);
    /// subscribes to default sink changes
    pub const DEFAULT_SOURCE_CHANGED: Self = Self(0b_0000_1000);
    /// subscribes to the list of sinks changing
    pub const SINKS_CHANGED: Self = Self(0b_0000_0001);
    /// subscribes to default sink's current profile changing
    pub const SINK_PROFILE_CHANGED: Self = Self(0b_0010_0000);
    /// subscribes to the list of sources changing
    pub const SOURCES_CHANGED: Self = Self(0b_0000_0100);
    /// subscribes to default sink's current profile changing
    pub const SOURCE_PROFILE_CHANGED: Self = Self(0b_0100_0000);

    pub fn is_set(&self, case: AudioRegisterData) -> bool {
        return *self & case != AudioRegisterData(0);
    }
}

impl AudioRegisterData {
    pub fn none() -> Self {
        Self(0)
    }

    pub fn all() -> Self {
        Self(0b0111_1111)
    }
}

impl BitOr for AudioRegisterData {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for AudioRegisterData {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl BitAnd for AudioRegisterData {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self::Output {
        Self(self.0 & rhs.0)
    }
}
