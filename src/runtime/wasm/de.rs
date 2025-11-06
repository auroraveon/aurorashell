//! deserializes bytes from a wasm module into a struct that represents
//! the data for those bytes
//!
//! all the structs that bytes can be deserialized into are found in
//! `crate::runtime::module`

pub trait Deserialize: Sized {
    fn deserialize(data: &[u8]) -> anyhow::Result<Self>;
}

use anyhow::anyhow;

use crate::runtime::module::{AudioRegisterData, Register};

impl Deserialize for Vec<Register> {
    fn deserialize(data: &[u8]) -> anyhow::Result<Self> {
        // must have at least 0x20 bytes for the header
        if data.len() < 0x10 {
            return Err(anyhow!(
                "[wasm] [Registers::deserialize] must have at least 0x10 bytes"
            ));
        }

        // shouldn't fail as we check for at least 0x10 bytes beforehand
        let num_registers: u16 = match data[0x06..0x08].try_into() {
            Ok(bytes) => u16::from_be_bytes(bytes),
            Err(err) => {
                return Err(anyhow!(
                    "[wasm] [Registers::deserialize] data[0x06..0x08] to [u8; 2] failed somehow: \
                     {}",
                    err
                ));
            }
        };

        // 0x10 is the size of each register entry in the table
        // and the extra + 0x10 is the offset to the start of the table
        let registers_table_end = 0x10 * num_registers as usize + 0x10;

        // can't allow a mismatch between size of data and amount of registers
        // that are said to be in the table
        if registers_table_end > data.len() {
            return Err(anyhow!(
                "[wasm] [Registers::deserialize] registers_table_end (0x{:02X}) > data length \
                 (0x{:02X})",
                registers_table_end,
                data.len()
            ));
        }

        let registers: Vec<Register> = data[0x10..registers_table_end]
            .chunks_exact(0x10)
            .map(|entry_bytes| Register::from_entry_bytes(data, entry_bytes, registers_table_end))
            .collect::<anyhow::Result<Vec<Register>>>()?;

        log::debug!("{:?}", registers);

        return Ok(registers);
    }
}

/// represents the unprocessed data from within each register entry
#[derive(Debug)]
struct RegisterEntryData {
    /// an integer that maps to a `Register`
    id: u16,
    /// each bit is a register
    ///
    /// 0 means unassigned and 1 is assigned
    ///
    /// the meaning of the registers are based on the id of the register
    registers: u32,
    /// a 32 bit integer that maps to an offset starting from the byte
    /// after the last register entry in the registers table
    ///
    /// not every register requires this to be set
    ///
    /// there is no special meaning for an offset of 0, it depends on the
    /// register used whether or not this offset needs to be read
    extra_data_offset: u32,
}

impl Register {
    fn from_entry_bytes(
        data: &[u8],
        entry_bytes: &[u8],
        extra_data_start: usize,
    ) -> anyhow::Result<Register> {
        let entry_bytes: [u8; 0x10] = match entry_bytes.try_into() {
            Ok(bytes) => bytes,
            Err(err) => {
                return Err(anyhow!(
                    "[wasm] [Registers] entry_bytes to [u8; 0x10] failed: {}\nbytes: {:?}",
                    err,
                    entry_bytes,
                ));
            }
        };

        let entry = Register::get_entry_data(entry_bytes)?;

        let res = match entry.id {
            1 => Register::PulseAudio {
                pulseaudio: AudioRegisterData(entry.registers as u8),
            },
            3 => {
                let offset = entry.extra_data_offset as usize + extra_data_start;
                // Interval's extra data is 0x10 bytes long
                let end = offset + 0x10;
                if offset > data.len() || end > data.len() {
                    return Err(anyhow!(
                        "[wasm] [Registers] Interval offsets out of bounds: {:02X}-{:02X}, data \
                         size: {:02X}",
                        offset,
                        end,
                        data.len(),
                    ));
                }

                let extra_data = &data[offset..end];

                let milliseconds: u64 = match extra_data[0x00..0x08].try_into() {
                    Ok(bytes) => u64::from_be_bytes(bytes),
                    Err(err) => {
                        return Err(anyhow!(
                            "[wasm] [Registers] Interval milliseconds to u64 failed: {}\nbytes: \
                             {:?}",
                            err,
                            extra_data,
                        ));
                    }
                };

                let offset: u32 = match extra_data[0x08..0x0C].try_into() {
                    Ok(bytes) => u32::from_be_bytes(bytes),
                    Err(err) => {
                        return Err(anyhow!(
                            "[wasm] [Registers] Interval offset to u64 failed: {}\nbytes: {:?}",
                            err,
                            extra_data,
                        ));
                    }
                };

                Register::Interval {
                    milliseconds,
                    offset,
                }
            }
            _ => {
                return Err(anyhow!("[wasm] [MODULE_HERE] value = {}", entry.id));
            }
        };

        return Ok(res);
    }

    /// takes a 0x10 byte array and converts it to a usable
    fn get_entry_data(bytes: [u8; 0x10]) -> anyhow::Result<RegisterEntryData> {
        Ok(RegisterEntryData {
            id: match bytes[0x00..0x02].try_into() {
                Ok(bytes) => u16::from_be_bytes(bytes),
                Err(err) => {
                    return Err(anyhow!(
                        "[wasm] [Registers] id entry bytes to u16 failed: {}\nbytes: {:?}",
                        err,
                        bytes,
                    ));
                }
            },
            registers: match bytes[0x02..0x06].try_into() {
                Ok(bytes) => u32::from_be_bytes(bytes),
                Err(err) => {
                    return Err(anyhow!(
                        "[wasm] [Registers] registers entry bytes to u32 failed: {}\nbytes: {:?}",
                        err,
                        bytes,
                    ));
                }
            },
            extra_data_offset: match bytes[0x06..0x0A].try_into() {
                Ok(bytes) => u32::from_be_bytes(bytes),
                Err(err) => {
                    return Err(anyhow!(
                        "[wasm] [Registers] extra_data_offset entry bytes to u32 failed: \
                         {}\nbytes: {:?}",
                        err,
                        bytes,
                    ));
                }
            },
        })
    }
}

////////////////////////////////////////////////////////////////////////////////
