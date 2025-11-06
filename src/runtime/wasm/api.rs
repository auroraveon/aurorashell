use wasmtime::{Caller, Linker};

use super::WasiContext;
use super::id::IdType;

/// links necessary functions for the modules
pub fn get_api_functions(linker: &mut Linker<WasiContext>) -> anyhow::Result<()> {
    // will only return 0 when an id type of None has been given
    linker.func_wrap(
        "env",
        "get_unique_id",
        |mut caller: Caller<'_, WasiContext>, id_type: u32| -> u32 {
            // note: cannot unwrap on this try_from!!! fix later
            // ~ aurora
            match IdType::try_from(id_type).unwrap() {
                IdType::None => 0,
                IdType::LayerSurface => {
                    let id = caller.data_mut().surface_wasm_id.unique();
                    id
                }
            }
        },
    )?;

    return Ok(());
}
