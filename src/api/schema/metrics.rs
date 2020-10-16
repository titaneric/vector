use crate::event::{Event, Metric, MetricValue};
use crate::metrics::{capture_metrics, get_controller, Controller};
use async_graphql::{validators::IntRange, Interface, Object, Subscription};
use async_stream::stream;
use chrono::{DateTime, Utc};
use lazy_static::lazy_static;
use std::sync::Arc;
use tokio::stream::{Stream, StreamExt};
use tokio::time::Duration;

lazy_static! {
    static ref GLOBAL_CONTROLLER: Arc<&'static Controller> =
        Arc::new(get_controller().expect("Metrics system not initialized. Please report."));
}

pub struct Uptime(Metric);

#[Object]
impl Uptime {
    /// Metric timestamp
    async fn timestamp(&self) -> Option<DateTime<Utc>> {
        self.0.timestamp
    }

    /// Number of seconds the Vector instance has been alive
    async fn seconds(&self) -> f64 {
        match self.0.value {
            MetricValue::Gauge { value } => value,
            _ => 0.00,
        }
    }
}

impl From<Metric> for Uptime {
    fn from(m: Metric) -> Self {
        Self(m)
    }
}

pub struct EventsProcessed(Metric);

#[Object]
impl EventsProcessed {
    /// Metric timestamp
    async fn timestamp(&self) -> Option<DateTime<Utc>> {
        self.0.timestamp
    }

    /// Number of events processed
    async fn events_processed(&self) -> f64 {
        match self.0.value {
            MetricValue::Counter { value } => value,
            _ => 0.00,
        }
    }
}

impl From<Metric> for EventsProcessed {
    fn from(m: Metric) -> Self {
        Self(m)
    }
}

pub struct BytesProcessed(Metric);

#[Object]
impl BytesProcessed {
    /// Metric timestamp
    async fn timestamp(&self) -> Option<DateTime<Utc>> {
        self.0.timestamp
    }

    /// Number of bytes processed
    async fn bytes_processed(&self) -> f64 {
        match self.0.value {
            MetricValue::Counter { value } => value,
            _ => 0.00,
        }
    }
}

impl From<Metric> for BytesProcessed {
    fn from(m: Metric) -> Self {
        Self(m)
    }
}

#[derive(Interface)]
#[graphql(field(name = "timestamp", type = "Option<DateTime<Utc>>"))]
pub enum MetricType {
    Uptime(Uptime),
    EventsProcessed(EventsProcessed),
    BytesProcessed(BytesProcessed),
}

#[derive(Default)]
pub struct MetricsSubscription;

#[Subscription]
impl MetricsSubscription {
    /// Metrics for how long the Vector instance has been running
    async fn uptime_metrics(
        &self,
        #[graphql(default = 1000, validator(IntRange(min = "100", max = "60_000")))] interval: i32,
    ) -> impl Stream<Item = Uptime> {
        get_metrics(interval).filter_map(|m| match m.name.as_str() {
            "uptime_seconds" => Some(Uptime(m)),
            _ => None,
        })
    }

    /// Events processed metrics
    async fn events_processed_metrics(
        &self,
        #[arg(default = 1000, validator(IntRange(min = "100", max = "60_000")))] interval: i32,
    ) -> impl Stream<Item = EventsProcessed> {
        get_metrics(interval).filter_map(|m| match m.name.as_str() {
            "events_processed" => Some(EventsProcessed(m)),
            _ => None,
        })
    }

    /// Bytes processed metrics
    async fn bytes_processed_metrics(
        &self,
        #[graphql(default = 1000, validator(IntRange(min = "100", max = "60_000")))] interval: i32,
    ) -> impl Stream<Item = BytesProcessed> {
        get_metrics(interval).filter_map(|m| match m.name.as_str() {
            "bytes_processed" => Some(BytesProcessed(m)),
            _ => None,
        })
    }

    /// All metrics
    async fn metrics(
        &self,
        #[graphql(default = 1000, validator(IntRange(min = "100", max = "60_000")))] interval: i32,
    ) -> impl Stream<Item = MetricType> {
        get_metrics(interval).filter_map(|m| match m.name.as_str() {
            "uptime_seconds" => Some(MetricType::Uptime(m.into())),
            "events_processed" => Some(MetricType::EventsProcessed(m.into())),
            "bytes_processed" => Some(MetricType::BytesProcessed(m.into())),
            _ => None,
        })
    }
}

/// Returns a stream of `Metric`s, collected at the provided millisecond interval
fn get_metrics(interval: i32) -> impl Stream<Item = Metric> {
    let controller = get_controller().unwrap();
    let mut interval = tokio::time::interval(Duration::from_millis(interval as u64));

    stream! {
        loop {
            interval.tick().await;
            for ev in capture_metrics(&controller) {
                if let Event::Metric(m) = ev {
                    yield m;
                }
            }
        }
    }
}

/// Get the events processed by topology component name
pub fn topology_events_processed(topology_name: String) -> Option<EventsProcessed> {
    let key = String::from("component_name");

    capture_metrics(&GLOBAL_CONTROLLER)
        .find(|ev| match ev {
            Event::Metric(m)
                if m.name.as_str().eq("events_processed")
                    && m.tag_matches(&key, &topology_name) =>
            {
                true
            }
            _ => false,
        })
        .map(|ev| EventsProcessed(ev.into_metric()))
}
