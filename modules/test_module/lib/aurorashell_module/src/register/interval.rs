use super::{IntoRegister, RegisterTrait};

/// requests for the wasm host to interrupt this module at the set interval
#[derive(Debug)]
pub struct Interval {
    /// the total amount of milliseconds combined for the interval
    ///
    /// this contains all milliseconds, seconds, minutes, hours, days added
    /// to the interval
    milliseconds: u64,
    /// shifts the interval interrupt forward
    ///
    /// the offset is in milliseconds
    offset: u32,
}

impl Interval {
    pub fn from_millis(millis: u64) -> Self {
        Self {
            milliseconds: millis,
            offset: 0,
        }
    }

    pub fn from_seconds(secs: u64) -> Self {
        Self {
            milliseconds: secs * 1000,
            offset: 0,
        }
    }

    pub fn from_minutes(mins: u64) -> Self {
        Self {
            milliseconds: mins * 1000 * 60,
            offset: 0,
        }
    }

    pub fn from_hours(hours: u64) -> Self {
        Self {
            milliseconds: hours * 1000 * 60 * 60,
            offset: 0,
        }
    }

    pub fn from_days(days: u64) -> Self {
        Self {
            milliseconds: days * 1000 * 60 * 60 * 24,
            offset: 0,
        }
    }

    pub fn add_millis(mut self, millis: u64) -> Self {
        self.milliseconds += millis;
        self
    }

    pub fn add_seconds(mut self, secs: u64) -> Self {
        self.milliseconds += secs * 1000;
        self
    }

    pub fn add_minutes(mut self, mins: u64) -> Self {
        self.milliseconds += mins * 1000 * 60;
        self
    }

    pub fn add_hours(mut self, hours: u64) -> Self {
        self.milliseconds += hours * 1000 * 60 * 60;
        self
    }

    pub fn add_days(mut self, days: u64) -> Self {
        self.milliseconds += days * 1000 * 60 * 60 * 24;
        self
    }

    /// adds an offset (in milliseconds) to shift the interval forward in time
    ///
    /// example:
    /// two `Interval`s can trigger every 5 seconds but one can be offset by
    /// 2 seconds to trigger 2 seconds later than the first
    pub fn offset(mut self, offset: u32) -> Self {
        self.offset = offset;
        self
    }
}

impl RegisterTrait for Interval {
    fn id(&self) -> u16 {
        Interval::const_id()
    }

    fn allow_duplicates(&self) -> bool {
        Interval::const_allow_duplicates()
    }

    fn registers(&self) -> u32 {
        0
    }

    fn serialize(&self) -> Option<Vec<u8>> {
        // 16 bytes (0x10) for extra data
        let mut bytes: [u8; 0x10] = [0; 0x10];

        let milliseconds_bytes: [u8; 0x08] = self.milliseconds.to_be_bytes();
        bytes[0x00..0x08].copy_from_slice(&milliseconds_bytes);

        let offset_bytes: [u8; 0x04] = self.offset.to_be_bytes();
        bytes[0x08..0x0C].copy_from_slice(&offset_bytes);

        return Some(bytes.to_vec());
    }
}

impl IntoRegister for Interval {}

// this stuff here is necessary for the macro to work as `const` isn't allowed
// it traits so i couldn't put them into `RegisterTrait`
//
// `into_register` is also put here instead of the trait so that we don't have
// to publically expose the
impl Interval {
    pub const fn const_id() -> u16 {
        0x00_03
    }

    pub const fn const_allow_duplicates() -> bool {
        true
    }
}
