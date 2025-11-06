use super::Event;

use crate::runtime::wasm::WasmSerializable;

impl WasmSerializable for Event {
    fn serialise(self) -> &'static [u8] {
        &[]
    }
}
