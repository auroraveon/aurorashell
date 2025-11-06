//! the service architecture is complex but flexible, allowing for various
//! implementations of a service while still keeping the same flow
//!
//! all services store their state internally but gives the main thread a
//! struct to interact with the service

pub mod audio;
//pub mod interval;

use std::fmt::Debug;

use iced::Subscription;
use iced::futures::channel::mpsc;

/// a service that provides data to modules
pub trait Service: Debug + Clone + Sized {
    /// type is emitted from the service when data changes
    type Event: Debug + Clone;
    /// type that is used to perform requests on the service
    type Request: Debug + Clone;
    /// state for the service
    type State: ServiceState<Self>;
    /// optional extra data for the service
    type RuntimeData: Debug;
    // note: add smth here please omg
    type RegisterData: Debug + Clone;

    /// allows the iced to subscribe to this service
    ///
    /// the subscription will emit `ServiceEvent::Init(Self)` on either:
    /// - on start
    /// - a crash
    ///
    /// example implementation:
    /// ```
    /// fn subscribe() -> iced::Subscription<ServiceEvent<Self>> {
    ///     // required for the subscription to work properly
    ///     let id = TypeId::of::<Self>();
    ///
    ///     Subscription::run_with_id(
    ///         id,
    ///         channel(64, async |mut chan| {
    ///             loop {
    ///                 // if the state is initialized outside of the loop, service state is
    ///                 // persistent across service restarts
    ///                 let mut state = ServiceState::init();
    ///
    ///                 // setup channel for modules to talk to this service
    ///                 let (tx, rx) = flume::bounded::<ServiceRequest<Self>>(64);
    ///
    ///                 // send channel to iced thread
    ///                 if let Err(err) = chan
    ///                     .send(ServiceEvent::Init {
    ///                         request_tx: tx,
    ///                     })
    ///                     .await
    ///                 {
    ///                     // could log and do something to handle this error and retry?
    ///                     // this would mean the service could not initalize properly
    ///                 }
    ///
    ///                 // start the service
    ///                 let err = Self::run(&mut state, &mut chan, &mut ()).await;
    ///
    ///                 // handle error or just log it
    ///             }
    ///         })
    ///     )
    /// }
    /// ```
    fn subscribe() -> Subscription<ServiceEvent<Self>>;

    /// runs the service
    ///
    /// should be called by `Self::subscribe`
    //
    // todo: make this private to anything but the services that need to implement
    // it
    async fn run(
        state: &mut Self::State,
        chan: &mut mpsc::Sender<ServiceEvent<Self>>,
        request_rx: flume::Receiver<ServiceRequest<Self>>,
        runtime_data: &mut Self::RuntimeData,
    ) -> anyhow::Error;
}

/// holds state for the service
pub trait ServiceState<S: Service>: Debug {
    fn init() -> Self;

    /// called when state needs to be updated
    ///
    /// extra events can be created and emitted from this function
    ///
    /// example implementation:
    /// ```
    /// fn update(&mut self, event: Event) -> Vec<Event> {
    ///     let mut _events = match event.clone() {
    ///         Event::Example { some_data } => {
    ///             // do whatever with data here like saving it
    ///             
    ///             vec![]
    ///         }
    ///     };
    ///
    ///     // this part is important, it ensures that the event passed into the function is
    ///     // actually emitted from the service
    ///     let mut events = vec![event];
    ///     events.append(&mut _events);
    ///     return events;
    /// }
    /// ```
    fn update(&mut self, event: S::Event) -> Vec<S::Event>;
}

/// ensures all services have a standard api for events
#[derive(Debug, Clone)]
pub enum ServiceEvent<S: Service> {
    /// when a service starts up or restarts, the service is expected
    /// to send a channel for requests to the service
    Init {
        /// the channel used to communicate with the service
        request_tx: flume::Sender<ServiceRequest<S>>,
    },
    /// all events must specify the runtime they're for, id, and event
    Update { event: S::Event },
}

/// ensures all services have a standard api for requests
#[derive(Debug, Clone)]
pub enum ServiceRequest<S: Service> {
    /// a request to the service
    Request { request: S::Request },
}
