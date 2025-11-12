use iced::runtime::platform_specific::wayland::layer_surface::SctkLayerSurfaceSettings;

use crate::runtime::wasm::{WasmCallbackData, WasmUiNode};
use crate::services::SubscriptionData;

/// messages that the wasm thread sends to the iced thread
#[derive(Debug, Clone)]
pub enum Event {
    /// representation of what a module returned from its view() function
    ///
    /// this gets sent the iced ui thread :3
    ModViewData {
        module_id: u32,
        surface_id: iced::window::Id,
        tree: Box<WasmUiNode>,
    },
    /// allows a wasm module to request for the iced thread to
    /// create a layer surface
    CreateLayerSurface(SctkLayerSurfaceSettings),
    /// allows a wasm module to request for the iced thread to
    /// destroy a layer surface
    DestroyLayerSurface(iced::window::Id),
    /// registers a module to a service, linking the items that the module
    /// wants to be aware of from the service
    RegisterModuleToService {
        module_id: u32,
        register: SubscriptionData,
    },
}

/// messages that the wasm thread receives from the iced thread
#[derive(Debug, Clone)]
pub enum Request {
    /// the ui thread sends this to the wasm thread when a callback
    /// was triggered for an element in the ui
    CallbackEvent {
        module_id: u32,
        surface_id: iced::window::Id,
        callback_id: u32,
        data: Option<WasmCallbackData>,
    },
}
