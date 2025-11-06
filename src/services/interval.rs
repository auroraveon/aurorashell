use crate::runtime::RuntimeModuleId;

use super::{ActiveService, PassiveService, ServiceChannel, ServiceEvent, ServiceRequest};

use std::{
    any::TypeId,
    time::{Duration, Instant},
};

use flume::Sender;
use iced::{Subscription, futures::SinkExt, stream::channel};

#[derive(Debug, Clone)]
pub enum Event {
    Event,
    Interrupt { id: u32, }
}

#[derive(Debug, Clone)]
pub enum Request {}

#[derive(Debug, Clone)]
pub struct IntervalState {
    pub channel: Sender<Request>,
}

impl ServiceChannel<IntervalService> for IntervalState {
    fn update(&mut self, event: Event) -> Option<Vec<Event>> {
        None
    }
}

#[derive(Debug, Clone)]
pub struct IntervalService;

impl PassiveService for IntervalService {
    type Event = Event;
    type State = IntervalState;

    fn subscribe() -> iced::Subscription<ServiceEvent<Self>> {
        let id = TypeId::of::<Self>();

        let start_time = Instant::now();
        let mut i = 1;
        let tps = 15;
        let delay_per_tick = (1000_f64 / tps as f64) as u64;
        let mut wait_time = Duration::from_millis(delay_per_tick);
        let mut last_drift: i128 = 0;

        Subscription::run_with_id(
            id,
            channel(100, async move |mut chan| {
                loop {
                    let execute_start = Instant::now();

                    //println!("aurora! :3");
                    match chan
                        .send(ServiceEvent::Update {
                            id: RuntimeModuleId::Wasm(0),
                            event: Event::Event,
                        })
                        .await
                    {
                        Ok(_) => (),
                        Err(err) => {
                            eprintln!("error sending thingy thing :3 | error: {}", err);
                        }
                    };

                    let execute_end = Instant::now();

                    tokio::time::sleep(wait_time).await;

                    let elapsed = start_time.elapsed().as_nanos() as i128;
                    let seconds = Duration::from_millis(delay_per_tick * i).as_nanos() as i128;

                    let execution_time = execute_end.duration_since(execute_start);

                    let time_drift = elapsed - seconds;

                    if time_drift > 0 {
                        // ahead of target time; positive time drift
                        wait_time = Duration::from_millis(delay_per_tick)
                            - Duration::from_nanos(time_drift as u64);

                        //wait_time -= execution_time;
                    } else {
                        // behind of target time; negative time drift
                        wait_time = Duration::from_millis(delay_per_tick)
                            + Duration::from_nanos((-time_drift) as u64);

                        //wait_time += execution_time;
                    }

                    let time_drift_ms = time_drift as f64 / 1_000_000_f64;
                    let drift_delta_ms = f64::abs((last_drift - time_drift) as f64 / 1_000_000_f64);
                    last_drift = time_drift;

                    log::debug!("{}ms", time_drift_ms);
                    log::debug!("{}Δms", drift_delta_ms);
                    log::debug!("{}µs", execution_time.as_micros());

                    i += 1;
                }
            }),
        )
    }
}

#[derive(Debug, Clone)]
pub struct RegisterData {
    pub milliseconds: u64,
    pub offset: u32,
}

impl ActiveService for IntervalService {
    type Request = Request;
    type RegisterData = RegisterData;

    fn request(state: &mut Self::State, request: ServiceRequest<Self>) -> anyhow::Result<()> {
        Ok(())
    }
}
