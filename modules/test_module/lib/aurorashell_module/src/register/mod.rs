mod interval;
mod pulseaudio;

use std::{collections::HashSet, fmt::Debug};

pub use interval::*;
pub use pulseaudio::*;

#[derive(Debug, Default)]
pub struct Registers {
    registers: Vec<Box<dyn RegisterTrait>>,
}

impl Registers {
    // only increase version when potential breaking changes have been made
    const SERIALIZED_VERSION: [u8; 2] = [0x00, 0x01];

    /// serializes `Registers` into a binary table of bitflags
    ///
    /// only serializes the registers that were selected
    /// any set to `None` will not be in the serialized table
    pub(crate) fn serialize(&self) -> Box<[u8]> {
        let mut serialized_bytes: Vec<u8> = vec![0; 0x10];

        // add the version
        serialized_bytes[0x04..0x06].copy_from_slice(&Self::SERIALIZED_VERSION);

        // add how many registers are in the table
        let n_registers_bytes: [u8; 0x02] = (self.registers.len() as u16).to_be_bytes();
        serialized_bytes[0x06..0x08].copy_from_slice(&n_registers_bytes);

        // this gets incremented as extra data is added
        let mut offset: u32 = 0;

        // keeps track of registers that have been seen already and aren't
        // allowed to have duplicates
        let mut seen: HashSet<u16> = HashSet::new();

        // bytes for the extra data 
        let mut extra_data: Vec<u8> = vec![];

        // adds the entry for the id per
        for register in &self.registers {
            let id = register.id();

            if seen.contains(&id) && !register.allow_duplicates() {
                // note: don't panic here just return an error silly :3
                panic!("detected duplicate register, id: {}", id);
            }

            seen.insert(id);

            // 16 bytes (0x10) per entry in the registers table
            let mut entry_bytes: [u8; 0x10] = [0; 0x10];

            let id_bytes = id.to_be_bytes();
            entry_bytes[0x00..0x02].copy_from_slice(&id_bytes);

            let registers_bytes = register.registers().to_be_bytes();
            entry_bytes[0x02..0x06].copy_from_slice(&registers_bytes);

            if let Some(extra_data_bytes) = register.serialize() {
                let offset_bytes: [u8; 0x04] = (offset).to_be_bytes();
                entry_bytes[0x06..0x0A].copy_from_slice(&offset_bytes);

                offset += extra_data_bytes.len() as u32;

                extra_data.extend(extra_data_bytes);
            }

            serialized_bytes.extend(entry_bytes);
        }

        // add the extra data to the output
        serialized_bytes.extend(extra_data);
        
        // then add the size of the bytes
        let size_bytes: [u8; 0x04] = (serialized_bytes.len() as u32).to_be_bytes();
        serialized_bytes[0x00..0x04].copy_from_slice(&size_bytes);

        return serialized_bytes.into_boxed_slice();
    }
}

impl Registers {
    pub fn new() -> Registers {
        Registers { registers: vec![] }
    }

    pub fn from_macro(registers: Vec<Register>) -> Self {
        Registers {
            registers: registers.into_iter().map(|boxed| boxed.0).collect(),
        }
    }
}

pub struct Register(pub(super) Box<dyn RegisterTrait>);

pub(super) trait RegisterTrait: Debug {
    /// an id of 0 is not allowed
    fn id(&self) -> u16;

    /// whether this register type allows multiple instances
    fn allow_duplicates(&self) -> bool;

    /// returns a 32 bit integer with bitflags for the register
    fn registers(&self) -> u32;

    /// returns optional extra data for the register
    fn serialize(&self) -> Option<Vec<u8>>;
}

pub trait IntoRegister: Debug + 'static {
    // we don't really care about this because all i'm trying to do is make it
    // easier to implement onto register structs
    //     - aurora :3
    #[allow(private_bounds)]
    fn into_register(self) -> Register
    where
        Self: Sized + RegisterTrait,
    {
        Register(Box::new(self) as Box<dyn RegisterTrait>)
    }
}
