pub mod module;
pub mod wasm;

use std::fmt::Debug;

use iced::{Subscription, Task};

use crate::app::AppMessage;

pub trait RuntimeState<R: RuntimeService>: Debug + Clone {
    /// when the Runtime emits an `ServiceEvent::Update(Self::Event)`,
    /// its received by the `App` which then calls this to update the State
    /// for the runtime
    ///
    /// requests are done similarly, but instead we call
    /// `Runtime::request(state, request)` as long as the Runtime implements
    /// `RuntimeService`
    fn update(&mut self, event: R::Event) -> Task<AppMessage>;
}

/// ensures all runtimes have a standard api for events
#[derive(Debug, Clone)]
pub enum RuntimeEvent<R: RuntimeService> {
    /// emitted and expected when a runtime starts up
    Init(R::State),
    /// events emitted from the runtime
    Update(R::Event),
}

/// ensures all runtimes have a standard api for requests
#[derive(Debug, Clone)]
pub enum RuntimeRequest<R: RuntimeService> {
    /// a request to the runtime
    Request { request: R::Request },
    /// data emitted from a service, that a module from a runtime requested
    /// through a register
    ServiceData { data: R::ServiceData },
}

pub trait RuntimeService: Debug + Clone + Sized {
    /// the init data
    type Init: Debug;

    /// event type of the runtime
    type Event: Debug + Clone + Send;
    /// request type of the runtime
    type Request: Debug + Clone + Send;

    /// the state for the runtime
    ///
    /// this is held by the iced app
    type State: Debug + Clone + RuntimeState<Self>;

    /// the data emit from a service's event
    type ServiceData: Debug + Send + Sync;

    /// allows the app to subscribe to this service
    ///
    /// the subscription will emit `ServiceEvent::Init(Self)` on either:
    /// - on start
    /// - a crash
    fn run(data: Self::Init) -> Subscription<RuntimeEvent<Self>>;

    /// a call to this function internally calls a channel on `Self::State`
    /// to send a request to the Runtime
    fn request(state: &mut Self::State, request: RuntimeRequest<Self>) -> anyhow::Result<()>;
}

/// an id that represents an id from a module in a particular runtime
///
/// makes it easier to know where a specific module
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum RuntimeModuleId {
    Wasm(u32),
}
