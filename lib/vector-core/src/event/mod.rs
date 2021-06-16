use buffers::bytes::{DecodeBytes, EncodeBytes};
use bytes::{Buf, BufMut, Bytes};
use chrono::{DateTime, SecondsFormat, Utc};
pub use finalization::{
    BatchNotifier, BatchStatus, BatchStatusReceiver, EventFinalizer, EventFinalizers, EventStatus,
};
pub use legacy_lookup::Lookup;
pub use log_event::LogEvent;
pub use metadata::{EventMetadata, WithMetadata};
pub use metric::{Metric, MetricKind, MetricValue, StatisticKind};
use prost::{DecodeError, EncodeError, Message};
use shared::EventDataEq;
use std::collections::{BTreeMap, HashMap};
use std::convert::{TryFrom, TryInto};
use std::fmt::Debug;
use std::sync::Arc;
use tracing::field::{Field, Visit};
pub use util::log::PathComponent;
pub use util::log::PathIter;
pub use value::Value;
#[cfg(feature = "vrl")]
pub use vrl_target::VrlTarget;

pub mod discriminant;
pub mod error;
mod finalization;
mod legacy_lookup;
mod log_event;
#[cfg(feature = "lua")]
pub mod lua;
pub mod merge_state;
mod metadata;
pub mod metric;
pub mod proto;
#[cfg(test)]
mod test;
pub mod util;
mod value;
#[cfg(feature = "vrl")]
mod vrl_target;

pub const PARTIAL: &str = "_partial";

#[derive(PartialEq, PartialOrd, Debug, Clone)]
pub enum Event {
    Log(LogEvent),
    Metric(Metric),
}

impl Event {
    #[must_use]
    pub fn new_empty_log() -> Self {
        Event::Log(LogEvent::default())
    }

    /// Return self as a `LogEvent`
    ///
    /// # Panics
    ///
    /// This function panics if self is anything other than an `Event::Log`.
    pub fn as_log(&self) -> &LogEvent {
        match self {
            Event::Log(log) => log,
            _ => panic!("Failed type coercion, {:?} is not a log event", self),
        }
    }

    /// Return self as a mutable `LogEvent`
    ///
    /// # Panics
    ///
    /// This function panics if self is anything other than an `Event::Log`.
    pub fn as_mut_log(&mut self) -> &mut LogEvent {
        match self {
            Event::Log(log) => log,
            _ => panic!("Failed type coercion, {:?} is not a log event", self),
        }
    }

    /// Coerces self into a `LogEvent`
    ///
    /// # Panics
    ///
    /// This function panics if self is anything other than an `Event::Log`.
    pub fn into_log(self) -> LogEvent {
        match self {
            Event::Log(log) => log,
            _ => panic!("Failed type coercion, {:?} is not a log event", self),
        }
    }

    /// Return self as a `Metric`
    ///
    /// # Panics
    ///
    /// This function panics if self is anything other than an `Event::Metric`.
    pub fn as_metric(&self) -> &Metric {
        match self {
            Event::Metric(metric) => metric,
            _ => panic!("Failed type coercion, {:?} is not a metric", self),
        }
    }

    /// Return self as a mutable `Metric`
    ///
    /// # Panics
    ///
    /// This function panics if self is anything other than an `Event::Metric`.
    pub fn as_mut_metric(&mut self) -> &mut Metric {
        match self {
            Event::Metric(metric) => metric,
            _ => panic!("Failed type coercion, {:?} is not a metric", self),
        }
    }

    /// Coerces self into `Metric`
    ///
    /// # Panics
    ///
    /// This function panics if self is anything other than an `Event::Metric`.
    pub fn into_metric(self) -> Metric {
        match self {
            Event::Metric(metric) => metric,
            _ => panic!("Failed type coercion, {:?} is not a metric", self),
        }
    }

    pub fn metadata(&self) -> &EventMetadata {
        match self {
            Self::Log(log) => log.metadata(),
            Self::Metric(metric) => metric.metadata(),
        }
    }

    pub fn metadata_mut(&mut self) -> &mut EventMetadata {
        match self {
            Self::Log(log) => log.metadata_mut(),
            Self::Metric(metric) => metric.metadata_mut(),
        }
    }

    /// Destroy the event and return the metadata.
    pub fn into_metadata(self) -> EventMetadata {
        match self {
            Self::Log(log) => log.into_parts().1,
            Self::Metric(metric) => metric.into_parts().2,
        }
    }

    pub fn add_batch_notifier(&mut self, batch: Arc<BatchNotifier>) {
        let finalizer = EventFinalizer::new(batch);
        match self {
            Self::Log(log) => log.add_finalizer(finalizer),
            Self::Metric(metric) => metric.add_finalizer(finalizer),
        }
    }

    pub fn with_batch_notifier(self, batch: &Arc<BatchNotifier>) -> Self {
        match self {
            Self::Log(log) => log.with_batch_notifier(batch).into(),
            Self::Metric(metric) => metric.with_batch_notifier(batch).into(),
        }
    }
}

impl EventDataEq for Event {
    fn event_data_eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Log(a), Self::Log(b)) => a.event_data_eq(b),
            (Self::Metric(a), Self::Metric(b)) => a.event_data_eq(b),
            _ => false,
        }
    }
}

fn timestamp_to_string(timestamp: &DateTime<Utc>) -> String {
    timestamp.to_rfc3339_opts(SecondsFormat::AutoSi, true)
}

impl From<&tracing::Event<'_>> for Event {
    fn from(event: &tracing::Event<'_>) -> Self {
        let now = chrono::Utc::now();
        let mut maker = MakeLogEvent::default();
        event.record(&mut maker);

        let mut log = maker.0;
        log.insert("timestamp", now);

        let meta = event.metadata();
        log.insert(
            "metadata.kind",
            if meta.is_event() {
                Value::Bytes("event".to_string().into())
            } else if meta.is_span() {
                Value::Bytes("span".to_string().into())
            } else {
                Value::Null
            },
        );
        log.insert("metadata.level", meta.level().to_string());
        log.insert(
            "metadata.module_path",
            meta.module_path()
                .map_or(Value::Null, |mp| Value::Bytes(mp.to_string().into())),
        );
        log.insert("metadata.target", meta.target().to_string());

        log.into()
    }
}

#[derive(Debug, Default)]
struct MakeLogEvent(LogEvent);

impl Visit for MakeLogEvent {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.insert(field.name(), value.to_string());
    }

    fn record_debug(&mut self, field: &Field, value: &dyn Debug) {
        self.0.insert(field.name(), format!("{:?}", value));
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.0.insert(field.name(), value);
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        let field = field.name();
        let converted: Result<i64, _> = value.try_into();
        match converted {
            Ok(value) => self.0.insert(field, value),
            Err(_) => self.0.insert(field, value.to_string()),
        };
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.0.insert(field.name(), value);
    }
}

impl From<BTreeMap<String, Value>> for Event {
    fn from(map: BTreeMap<String, Value>) -> Self {
        Self::Log(LogEvent::from(map))
    }
}

impl From<HashMap<String, Value>> for Event {
    fn from(map: HashMap<String, Value>) -> Self {
        Self::Log(LogEvent::from(map))
    }
}

impl TryFrom<serde_json::Value> for Event {
    type Error = crate::Error;

    fn try_from(map: serde_json::Value) -> Result<Self, Self::Error> {
        match map {
            serde_json::Value::Object(fields) => Ok(Event::from(
                fields
                    .into_iter()
                    .map(|(k, v)| (k, v.into()))
                    .collect::<BTreeMap<_, _>>(),
            )),
            _ => Err(crate::Error::from(
                "Attempted to convert non-Object JSON into an Event.",
            )),
        }
    }
}

impl TryInto<serde_json::Value> for Event {
    type Error = serde_json::Error;

    fn try_into(self) -> Result<serde_json::Value, Self::Error> {
        match self {
            Event::Log(fields) => serde_json::to_value(fields),
            Event::Metric(metric) => serde_json::to_value(metric),
        }
    }
}

impl From<proto::StatisticKind> for StatisticKind {
    fn from(kind: proto::StatisticKind) -> Self {
        match kind {
            proto::StatisticKind::Histogram => StatisticKind::Histogram,
            proto::StatisticKind::Summary => StatisticKind::Summary,
        }
    }
}

impl From<metric::Sample> for proto::DistributionSample {
    fn from(sample: metric::Sample) -> Self {
        Self {
            value: sample.value,
            rate: sample.rate,
        }
    }
}

impl From<proto::DistributionSample> for metric::Sample {
    fn from(sample: proto::DistributionSample) -> Self {
        Self {
            value: sample.value,
            rate: sample.rate,
        }
    }
}

impl From<metric::Bucket> for proto::HistogramBucket {
    fn from(bucket: metric::Bucket) -> Self {
        Self {
            upper_limit: bucket.upper_limit,
            count: bucket.count,
        }
    }
}

impl From<proto::HistogramBucket> for metric::Bucket {
    fn from(bucket: proto::HistogramBucket) -> Self {
        Self {
            upper_limit: bucket.upper_limit,
            count: bucket.count,
        }
    }
}

impl From<metric::Quantile> for proto::SummaryQuantile {
    fn from(quantile: metric::Quantile) -> Self {
        Self {
            upper_limit: quantile.upper_limit,
            value: quantile.value,
        }
    }
}

impl From<proto::SummaryQuantile> for metric::Quantile {
    fn from(quantile: proto::SummaryQuantile) -> Self {
        Self {
            upper_limit: quantile.upper_limit,
            value: quantile.value,
        }
    }
}

impl From<Bytes> for Event {
    fn from(message: Bytes) -> Self {
        Event::Log(LogEvent::from(message))
    }
}

impl From<&str> for Event {
    fn from(line: &str) -> Self {
        LogEvent::from(line).into()
    }
}

impl From<String> for Event {
    fn from(line: String) -> Self {
        LogEvent::from(line).into()
    }
}

impl From<LogEvent> for Event {
    fn from(log: LogEvent) -> Self {
        Event::Log(log)
    }
}

impl From<Metric> for Event {
    fn from(metric: Metric) -> Self {
        Event::Metric(metric)
    }
}

/// A wrapper for references to inner event types, where reconstituting
/// a full `Event` from a `LogEvent` or `Metric` might be inconvenient.
#[derive(Clone, Copy, Debug)]
pub enum EventRef<'a> {
    Log(&'a LogEvent),
    Metric(&'a Metric),
}

impl<'a> From<&'a Event> for EventRef<'a> {
    fn from(event: &'a Event) -> Self {
        match event {
            Event::Log(log) => log.into(),
            Event::Metric(metric) => metric.into(),
        }
    }
}

impl<'a> From<&'a LogEvent> for EventRef<'a> {
    fn from(log: &'a LogEvent) -> Self {
        Self::Log(log)
    }
}

impl<'a> From<&'a Metric> for EventRef<'a> {
    fn from(metric: &'a Metric) -> Self {
        Self::Metric(metric)
    }
}

impl EncodeBytes<Event> for Event {
    type Error = EncodeError;

    fn encode<B>(self, buffer: &mut B) -> Result<(), Self::Error>
    where
        B: BufMut,
    {
        proto::EventWrapper::from(self).encode(buffer)
    }
}

impl DecodeBytes<Event> for Event {
    type Error = DecodeError;

    fn decode<B>(buffer: B) -> Result<Event, Self::Error>
    where
        B: Buf,
    {
        proto::EventWrapper::decode(buffer).map(|wrp| wrp.into())
    }
}
