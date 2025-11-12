//! the service architecture is complex but flexible, allowing for various
//! implementations of a service while still keeping the same flow
//!
//! all services store their state internally but gives the main thread a
//! struct to interact with the service

pub mod audio;
//pub mod interval;

use crate::runtime::RuntimeModuleId;
use crate::services::audio::AudioSubscriptionData;

use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::hash::Hash;

use iced::Subscription;
use iced::futures::channel::mpsc;

/// a service that provides data to modules
pub trait Service: Debug + Clone + Sized {
    /// state for the service
    type State: ServiceState<Self>;
    /// optional extra data for the service
    type RuntimeData: Debug;

    /// type is emitted from the service when data changes
    type Event: Debug + Clone;
    /// type that is used to perform requests on the service
    type Request: Debug + Clone;
    /// representation of the raw data from a module used to subscribe/listen
    /// to events from the service
    // note: probably not needed, will look into this
    // - aurora :3
    type SubscriptionData: Debug + Clone;
    /// holds the same enums as `Self::Event`, just without the contained data
    /// this is used for the service's `ModuleIds` so we know which modules to
    /// send an `Self::Event` to
    type EventType: Debug + Clone + Hash + Eq + PartialEq;

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
    ///             // services need to be aware of modules even after a service restart so we put
    ///             // it outside the loop to make it persistent
    ///             let mut module_ids = ModuleIds::new();
    ///
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
    ///                 let err = Self::run(&mut state, &mut chan, rx, &mut ()).await;
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
        module_ids: &mut ModuleIds<Self>,
        runtime_data: &mut Self::RuntimeData,
        chan: &mut mpsc::Sender<ServiceEvent<Self>>,
        request_rx: flume::Receiver<ServiceRequest<Self>>,
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
    /// a request to register a module to the service
    SubscribeModule {
        /// the id of the module in a runtime
        id: RuntimeModuleId,
        /// see `Service::SubscriptionData`
        data: S::SubscriptionData,
    },
}

/// data structure for storing the relationship between module ids and
/// the events they registered for
#[derive(Debug)]
pub struct ModuleIds<S: Service> {
    /// used for when an event has occured and we need to emit that event
    /// to all registered modules
    events_to_ids: HashMap<S::EventType, HashSet<RuntimeModuleId>>,
    /// used for when we need to remove a module
    ids_to_events: HashMap<RuntimeModuleId, HashSet<S::EventType>>,
}

impl<S: Service> ModuleIds<S> {
    pub fn new() -> Self {
        ModuleIds {
            events_to_ids: HashMap::new(),
            ids_to_events: HashMap::new(),
        }
    }

    /// registers a module with the service
    pub fn register_module(&mut self, id: RuntimeModuleId, events: Vec<S::EventType>) {
        for event in &events {
            if let Some(ids) = &mut self.events_to_ids.get_mut(event) {
                ids.insert(id.clone());
            } else {
                let mut ids = HashSet::new();
                ids.insert(id.clone());
                self.events_to_ids.insert(event.clone(), ids);
            }
        }
        self.ids_to_events.insert(id, HashSet::from_iter(events));
    }

    /// unregisters a module from the service
    pub fn unregister_module(&mut self, id: RuntimeModuleId) {
        let events = match self.ids_to_events.remove(&id) {
            Some(events) => events,
            None => return,
        };

        for event in &events {
            if let Some(ids) = self.events_to_ids.get_mut(event) {
                ids.remove(&id);
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum SubscriptionData {
    Interval { milliseconds: u64, offset: u32 },
    PulseAudio { data: AudioSubscriptionData },
}
