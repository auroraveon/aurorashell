use std::collections::HashMap;

use anyhow::anyhow;
use iced::window::Id;

/// generates unique ids for a wasm module
#[derive(Debug)]
pub struct WasmId {
    /// always incremented up by 1, starting from 1
    ///
    /// 0 means none / out of ids (when u8::MAX is reached)
    ///
    /// we use a u32 because thats what gets passed to the module anyway
    current_id: u32,
    /// tracks currently leased ids
    ///
    /// each leased id for the current module gets mapped to a
    /// unique `iced::window::Id`
    ///
    /// dropped ids cannot be reused
    leased_ids: HashMap<u32, Id>,
    /// a lookup table that maps `iced::window::Id` to back to u32
    ///
    /// this is the reverse of `Self::leased_ids` and is here
    /// to make lookups of Ids quicker
    iced_id_lut: HashMap<Id, u32>,
}

impl WasmId {
    /// gets a unique id
    ///
    /// 0 means none or out of ids (when u8::MAX is reached)
    ///
    /// a u32 is used because thats what the wasm module expects
    pub fn unique(&mut self) -> u32 {
        // makes sure it can never return 0 but to ensure that it starts
        // at 1 the default value must return 0
        if self.current_id == 0 {
            self.current_id += 1;
        } else if self.current_id == u8::MAX as u32 {
            return 0;
        }

        let id = self.current_id;
        let iced_id = Id::unique();

        self.leased_ids.insert(id, iced_id);
        self.iced_id_lut.insert(iced_id, id);

        self.current_id += 1;

        id
    }

    /// checks if the id has been leased
    pub fn has_lease(&self, id: u32) -> bool {
        self.leased_ids.contains_key(&id)
    }

    /// returns true if it found and dropped the id successfully
    ///
    /// dropped ids cannot be reused
    pub fn drop_id(&mut self, id: u32) -> bool {
        if let Some(_) = self.leased_ids.remove(&id) {
            true
        } else {
            false
        }
    }

    /// get the `iced::window::Id` that was generated
    pub fn get_iced_id(&self, id: &u32) -> Option<&iced::window::Id> {
        self.leased_ids.get(id)
    }

    /// get the u32 that corresponds to a `iced::window::Id`
    pub fn get_id(&self, id: &Id) -> Option<&u32> {
        self.iced_id_lut.get(id)
    }

    /// gets a vec of all the currently leased ids
    pub fn get_ids(&self) -> Vec<&u32> {
        self.leased_ids.keys().collect()
    }
}

impl Default for WasmId {
    fn default() -> Self {
        WasmId {
            current_id: 1,
            leased_ids: HashMap::new(),
            iced_id_lut: HashMap::new(),
        }
    }
}

/// represents the id type of
#[repr(u32)]
#[derive(Debug)]
pub enum IdType {
    None,
    LayerSurface,
}

impl TryFrom<u32> for IdType {
    type Error = anyhow::Error;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        return Ok(match value {
            0 => IdType::None,
            1 => IdType::LayerSurface,
            _ => return Err(anyhow!("IdType::try_from failed, value: {}", value)),
        });
    }
}
