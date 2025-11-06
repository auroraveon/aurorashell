use super::{Event, WasmRuntime, WasmUiNode};

use crate::app::AppMessage;
use crate::runtime::{RuntimeRequest, RuntimeService, RuntimeState};

use std::collections::HashMap;

use iced::Task;
use iced::platform_specific::shell::commands::layer_surface::{
    destroy_layer_surface, get_layer_surface,
};
use iced::window::Id;

#[derive(Debug, Clone)]
pub struct WasmState {
    /// used to send requests to the `WasmService`
    pub(super) channel: flume::Sender<RuntimeRequest<WasmRuntime>>,

    /// maps module ids to a map of surface ids to ui trees
    pub module_ui_trees: HashMap<u32, HashMap<Id, Box<WasmUiNode>>>,
    /// maps surface ids to module ids
    ///
    /// used as a lookup table for `Self::module_ui_trees`
    pub surface_module_ids: HashMap<Id, u32>,
}

impl RuntimeState<WasmRuntime> for WasmState {
    fn update(&mut self, event: <WasmRuntime as RuntimeService>::Event) -> Task<AppMessage> {
        match event {
            Event::ModViewData {
                module_id,
                surface_id,
                tree,
            } => {
                self.surface_module_ids.insert(surface_id, module_id);
                if let Some(map) = self.module_ui_trees.get_mut(&module_id) {
                    map.insert(surface_id, tree);
                } else {
                    let mut map = HashMap::new();
                    map.insert(surface_id, tree);
                    self.module_ui_trees.insert(module_id, map);
                }
            }
            Event::CreateLayerSurface(layer) => {
                return get_layer_surface(layer);
            }
            Event::DestroyLayerSurface(layer) => {
                return destroy_layer_surface(layer);
            }
            _ => {}
        };

        return Task::none();
    }
}
