use self::proto::{event_wrapper::Event as EventProto, metric::Value as MetricProto, Log};
use crate::config::log_schema;
use bytes::Bytes;
use chrono::{DateTime, SecondsFormat, TimeZone, Utc};
use std::collections::{BTreeMap, HashMap};
use std::iter::FromIterator;

pub mod discriminant;
pub mod merge;
pub mod merge_state;
pub mod metric;
pub mod util;

mod log_event;
mod lookup;
mod value;

pub use log_event::LogEvent;
pub use lookup::Lookup;
pub use metric::{Metric, MetricKind, MetricValue, StatisticKind};
use std::convert::{TryFrom, TryInto};
pub(crate) use util::log::PathComponent;
pub(crate) use util::log::PathIter;
pub use value::Value;

pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/event.proto.rs"));
}

pub const PARTIAL: &str = "_partial";

#[derive(PartialEq, Debug, Clone)]
pub enum Event {
    Log(LogEvent),
    Metric(Metric),
}

impl Event {
    pub fn new_empty_log() -> Self {
        Event::Log(LogEvent::default())
    }

    pub fn as_log(&self) -> &LogEvent {
        match self {
            Event::Log(log) => log,
            _ => panic!("Failed type coercion, {:?} is not a log event", self),
        }
    }

    pub fn as_mut_log(&mut self) -> &mut LogEvent {
        match self {
            Event::Log(log) => log,
            _ => panic!("Failed type coercion, {:?} is not a log event", self),
        }
    }

    pub fn into_log(self) -> LogEvent {
        match self {
            Event::Log(log) => log,
            _ => panic!("Failed type coercion, {:?} is not a log event", self),
        }
    }

    pub fn as_metric(&self) -> &Metric {
        match self {
            Event::Metric(metric) => metric,
            _ => panic!("Failed type coercion, {:?} is not a metric", self),
        }
    }

    pub fn as_mut_metric(&mut self) -> &mut Metric {
        match self {
            Event::Metric(metric) => metric,
            _ => panic!("Failed type coercion, {:?} is not a metric", self),
        }
    }

    pub fn into_metric(self) -> Metric {
        match self {
            Event::Metric(metric) => metric,
            _ => panic!("Failed type coercion, {:?} is not a metric", self),
        }
    }
}

fn timestamp_to_string(timestamp: &DateTime<Utc>) -> String {
    timestamp.to_rfc3339_opts(SecondsFormat::AutoSi, true)
}

fn decode_map(fields: BTreeMap<String, proto::Value>) -> Option<Value> {
    let mut accum: BTreeMap<String, Value> = BTreeMap::new();
    for (key, value) in fields {
        match decode_value(value) {
            Some(value) => {
                accum.insert(key, value);
            }
            None => return None,
        }
    }
    Some(Value::Map(accum))
}

fn decode_array(items: Vec<proto::Value>) -> Option<Value> {
    let mut accum = Vec::with_capacity(items.len());
    for value in items {
        match decode_value(value) {
            Some(value) => accum.push(value),
            None => return None,
        }
    }
    Some(Value::Array(accum))
}

fn decode_value(input: proto::Value) -> Option<Value> {
    match input.kind {
        Some(proto::value::Kind::RawBytes(data)) => Some(Value::Bytes(data.into())),
        Some(proto::value::Kind::Timestamp(ts)) => Some(Value::Timestamp(
            chrono::Utc.timestamp(ts.seconds, ts.nanos as u32),
        )),
        Some(proto::value::Kind::Integer(value)) => Some(Value::Integer(value)),
        Some(proto::value::Kind::Float(value)) => Some(Value::Float(value)),
        Some(proto::value::Kind::Boolean(value)) => Some(Value::Boolean(value)),
        Some(proto::value::Kind::Map(map)) => decode_map(map.fields),
        Some(proto::value::Kind::Array(array)) => decode_array(array.items),
        Some(proto::value::Kind::Null(_)) => Some(Value::Null),
        None => {
            error!("Encoded event contains unknown value kind.");
            None
        }
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

impl From<proto::EventWrapper> for Event {
    fn from(proto: proto::EventWrapper) -> Self {
        let event = proto.event.unwrap();

        match event {
            EventProto::Log(proto) => {
                let fields = proto
                    .fields
                    .into_iter()
                    .filter_map(|(k, v)| decode_value(v).map(|value| (k, value)))
                    .collect::<BTreeMap<_, _>>();

                Event::Log(LogEvent::from(fields))
            }
            EventProto::Metric(proto) => {
                let kind = match proto.kind() {
                    proto::metric::Kind::Incremental => MetricKind::Incremental,
                    proto::metric::Kind::Absolute => MetricKind::Absolute,
                };

                let name = proto.name;

                let namespace = if proto.namespace.is_empty() {
                    None
                } else {
                    Some(proto.namespace)
                };

                let timestamp = proto
                    .timestamp
                    .map(|ts| chrono::Utc.timestamp(ts.seconds, ts.nanos as u32));

                let tags = if !proto.tags.is_empty() {
                    Some(proto.tags)
                } else {
                    None
                };

                let value = match proto.value.unwrap() {
                    MetricProto::Counter(counter) => MetricValue::Counter {
                        value: counter.value,
                    },
                    MetricProto::Gauge(gauge) => MetricValue::Gauge { value: gauge.value },
                    MetricProto::Set(set) => MetricValue::Set {
                        values: set.values.into_iter().collect(),
                    },
                    MetricProto::Distribution(dist) => MetricValue::Distribution {
                        statistic: match dist.statistic() {
                            proto::distribution::StatisticKind::Histogram => {
                                StatisticKind::Histogram
                            }
                            proto::distribution::StatisticKind::Summary => StatisticKind::Summary,
                        },
                        values: dist.values,
                        sample_rates: dist.sample_rates,
                    },
                    MetricProto::AggregatedHistogram(hist) => MetricValue::AggregatedHistogram {
                        buckets: hist.buckets,
                        counts: hist.counts,
                        count: hist.count,
                        sum: hist.sum,
                    },
                    MetricProto::AggregatedSummary(summary) => MetricValue::AggregatedSummary {
                        quantiles: summary.quantiles,
                        values: summary.values,
                        count: summary.count,
                        sum: summary.sum,
                    },
                };

                Event::Metric(Metric {
                    name,
                    namespace,
                    timestamp,
                    tags,
                    kind,
                    value,
                })
            }
        }
    }
}

fn encode_value(value: Value) -> proto::Value {
    proto::Value {
        kind: match value {
            Value::Bytes(b) => Some(proto::value::Kind::RawBytes(b.to_vec())),
            Value::Timestamp(ts) => Some(proto::value::Kind::Timestamp(prost_types::Timestamp {
                seconds: ts.timestamp(),
                nanos: ts.timestamp_subsec_nanos() as i32,
            })),
            Value::Integer(value) => Some(proto::value::Kind::Integer(value)),
            Value::Float(value) => Some(proto::value::Kind::Float(value)),
            Value::Boolean(value) => Some(proto::value::Kind::Boolean(value)),
            Value::Map(fields) => Some(proto::value::Kind::Map(encode_map(fields))),
            Value::Array(items) => Some(proto::value::Kind::Array(encode_array(items))),
            Value::Null => Some(proto::value::Kind::Null(proto::ValueNull::NullValue as i32)),
        },
    }
}

fn encode_map(fields: BTreeMap<String, Value>) -> proto::ValueMap {
    proto::ValueMap {
        fields: fields
            .into_iter()
            .map(|(key, value)| (key, encode_value(value)))
            .collect(),
    }
}

fn encode_array(items: Vec<Value>) -> proto::ValueArray {
    proto::ValueArray {
        items: items.into_iter().map(encode_value).collect(),
    }
}

impl From<Event> for proto::EventWrapper {
    fn from(event: Event) -> Self {
        match event {
            Event::Log(log_event) => {
                let fields = log_event
                    .into_iter()
                    .map(|(k, v)| (k, encode_value(v)))
                    .collect::<BTreeMap<_, _>>();

                let event = EventProto::Log(Log { fields });

                proto::EventWrapper { event: Some(event) }
            }
            Event::Metric(Metric {
                name,
                namespace,
                timestamp,
                tags,
                kind,
                value,
            }) => {
                let namespace = namespace.unwrap_or_default();

                let timestamp = timestamp.map(|ts| prost_types::Timestamp {
                    seconds: ts.timestamp(),
                    nanos: ts.timestamp_subsec_nanos() as i32,
                });

                let tags = tags.unwrap_or_default();

                let kind = match kind {
                    MetricKind::Incremental => proto::metric::Kind::Incremental,
                    MetricKind::Absolute => proto::metric::Kind::Absolute,
                }
                .into();

                let metric = match value {
                    MetricValue::Counter { value } => {
                        MetricProto::Counter(proto::Counter { value })
                    }
                    MetricValue::Gauge { value } => MetricProto::Gauge(proto::Gauge { value }),
                    MetricValue::Set { values } => MetricProto::Set(proto::Set {
                        values: values.into_iter().collect(),
                    }),
                    MetricValue::Distribution {
                        values,
                        sample_rates,
                        statistic,
                    } => MetricProto::Distribution(proto::Distribution {
                        values,
                        sample_rates,
                        statistic: match statistic {
                            StatisticKind::Histogram => {
                                proto::distribution::StatisticKind::Histogram
                            }
                            StatisticKind::Summary => proto::distribution::StatisticKind::Summary,
                        }
                        .into(),
                    }),
                    MetricValue::AggregatedHistogram {
                        buckets,
                        counts,
                        count,
                        sum,
                    } => MetricProto::AggregatedHistogram(proto::AggregatedHistogram {
                        buckets,
                        counts,
                        count,
                        sum,
                    }),
                    MetricValue::AggregatedSummary {
                        quantiles,
                        values,
                        count,
                        sum,
                    } => MetricProto::AggregatedSummary(proto::AggregatedSummary {
                        quantiles,
                        values,
                        count,
                        sum,
                    }),
                };

                let event = EventProto::Metric(proto::Metric {
                    name,
                    namespace,
                    timestamp,
                    tags,
                    kind,
                    value: Some(metric),
                });

                proto::EventWrapper { event: Some(event) }
            }
        }
    }
}

impl From<Bytes> for Event {
    fn from(message: Bytes) -> Self {
        let mut event = Event::Log(LogEvent::from(BTreeMap::new()));

        event
            .as_mut_log()
            .insert(log_schema().message_key(), message);
        event
            .as_mut_log()
            .insert(log_schema().timestamp_key(), Utc::now());

        event
    }
}

impl From<&str> for Event {
    fn from(line: &str) -> Self {
        line.to_owned().into()
    }
}

impl From<String> for Event {
    fn from(line: String) -> Self {
        Bytes::from(line).into()
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

impl remap::Object for Event {
    fn get(&self, path: &remap::Path) -> Result<Option<remap::Value>, String> {
        if path.is_root() {
            let iter = self
                .as_log()
                .as_map()
                .clone()
                .into_iter()
                .map(|(k, v)| (k, v.into()));

            return Ok(Some(remap::Value::from_iter(iter)));
        }

        let value = path
            .to_alternative_strings()
            .iter()
            .find_map(|key| self.as_log().get(key))
            .cloned()
            .map(Into::into);

        Ok(value)
    }

    fn remove(&mut self, path: &remap::Path, compact: bool) -> Result<(), String> {
        if path.is_root() {
            for key in self.as_log().keys().collect::<Vec<_>>() {
                self.as_mut_log().remove_prune(key, compact);
            }

            return Ok(());
        };

        // loop until we find a path that exists.
        for key in path.to_alternative_strings() {
            if !self.as_log().contains(&key) {
                continue;
            }

            self.as_mut_log().remove_prune(&key, compact);
            break;
        }

        Ok(())
    }

    fn insert(&mut self, path: &remap::Path, value: remap::Value) -> Result<(), String> {
        if path.is_root() {
            match value {
                remap::Value::Map(map) => {
                    *self = map
                        .into_iter()
                        .map(|(k, v)| (k, v.into()))
                        .collect::<BTreeMap<_, _>>()
                        .into();

                    return Ok(());
                }
                _ => return Err("tried to assign non-map value to event root path".to_owned()),
            }
        }

        if let Some(path) = path.to_alternative_strings().first() {
            self.as_mut_log().insert(path, value);
        }

        Ok(())
    }

    fn paths(&self) -> Result<Vec<remap::Path>, String> {
        if self.as_log().is_empty() {
            return Ok(vec![remap::Path::root()]);
        }

        self.as_log()
            .keys()
            .map(|key| remap::Path::from_alternative_string(key).map_err(|err| err.to_string()))
            .collect::<Result<Vec<_>, String>>()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use regex::Regex;
    use std::collections::HashSet;

    #[test]
    fn serialization() {
        let mut event = Event::from("raw log line");
        event.as_mut_log().insert("foo", "bar");
        event.as_mut_log().insert("bar", "baz");

        let expected_all = serde_json::json!({
            "message": "raw log line",
            "foo": "bar",
            "bar": "baz",
            "timestamp": event.as_log().get(log_schema().timestamp_key()),
        });

        let actual_all = serde_json::to_value(event.as_log().all_fields()).unwrap();
        assert_eq!(expected_all, actual_all);

        let rfc3339_re = Regex::new(r"\A\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d+Z\z").unwrap();
        assert!(rfc3339_re.is_match(actual_all.pointer("/timestamp").unwrap().as_str().unwrap()));
    }

    #[test]
    fn type_serialization() {
        use serde_json::json;

        let mut event = Event::from("hello world");
        event.as_mut_log().insert("int", 4);
        event.as_mut_log().insert("float", 5.5);
        event.as_mut_log().insert("bool", true);
        event.as_mut_log().insert("string", "thisisastring");

        let map = serde_json::to_value(event.as_log().all_fields()).unwrap();
        assert_eq!(map["float"], json!(5.5));
        assert_eq!(map["int"], json!(4));
        assert_eq!(map["bool"], json!(true));
        assert_eq!(map["string"], json!("thisisastring"));
    }

    #[test]
    fn event_iteration() {
        let mut event = Event::new_empty_log();

        event
            .as_mut_log()
            .insert("Ke$ha", "It's going down, I'm yelling timber");
        event
            .as_mut_log()
            .insert("Pitbull", "The bigger they are, the harder they fall");

        let all = event
            .as_log()
            .all_fields()
            .map(|(k, v)| (k, v.to_string_lossy()))
            .collect::<HashSet<_>>();
        assert_eq!(
            all,
            vec![
                (
                    String::from("Ke$ha"),
                    "It's going down, I'm yelling timber".to_string()
                ),
                (
                    String::from("Pitbull"),
                    "The bigger they are, the harder they fall".to_string()
                ),
            ]
            .into_iter()
            .collect::<HashSet<_>>()
        );
    }

    #[test]
    fn event_iteration_order() {
        let mut event = Event::new_empty_log();
        let log = event.as_mut_log();
        log.insert("lZDfzKIL", Value::from("tOVrjveM"));
        log.insert("o9amkaRY", Value::from("pGsfG7Nr"));
        log.insert("YRjhxXcg", Value::from("nw8iM5Jr"));

        let collected: Vec<_> = log.all_fields().collect();
        assert_eq!(
            collected,
            vec![
                (String::from("YRjhxXcg"), &Value::from("nw8iM5Jr")),
                (String::from("lZDfzKIL"), &Value::from("tOVrjveM")),
                (String::from("o9amkaRY"), &Value::from("pGsfG7Nr")),
            ]
        );
    }

    #[test]
    fn object_get() {
        use remap::{map, Field::*, Object, Path, Segment::*};

        let cases = vec![
            (map![], vec![], Ok(Some(map![].into()))),
            (
                map!["foo": "bar"],
                vec![],
                Ok(Some(map!["foo": "bar"].into())),
            ),
            (
                map!["foo": "bar"],
                vec![Field(Regular("foo".to_owned()))],
                Ok(Some("bar".into())),
            ),
            (
                map!["foo": "bar"],
                vec![Field(Regular("bar".to_owned()))],
                Ok(None),
            ),
            (
                map!["foo": vec![map!["bar": true]]],
                vec![
                    Field(Regular("foo".to_owned())),
                    Index(0),
                    Field(Regular("bar".to_owned())),
                ],
                Ok(Some(true.into())),
            ),
            (
                map!["foo": map!["bar baz": map!["baz": 2]]],
                vec![
                    Field(Regular("foo".to_owned())),
                    Coalesce(vec![
                        Regular("qux".to_owned()),
                        Quoted("bar baz".to_owned()),
                    ]),
                    Field(Regular("baz".to_owned())),
                ],
                Ok(Some(2.into())),
            ),
        ];

        for (value, segments, expect) in cases {
            let value: BTreeMap<String, Value> = value;
            let event = Event::from(value);
            let path = Path::new_unchecked(segments);

            assert_eq!(event.get(&path), expect)
        }
    }

    #[test]
    fn object_insert() {
        use remap::{map, Field::*, Object, Path, Segment::*};

        let cases = vec![
            (
                map!["foo": "bar"],
                vec![],
                map!["baz": "qux"].into(),
                map!["baz": "qux"],
                Ok(()),
            ),
            (
                map!["foo": "bar"],
                vec![Field(Regular("foo".to_owned()))],
                "baz".into(),
                map!["foo": "baz"],
                Ok(()),
            ),
            (
                map!["foo": "bar"],
                vec![
                    Field(Regular("foo".to_owned())),
                    Index(2),
                    Field(Quoted("bar baz".to_owned())),
                    Field(Regular("a".to_owned())),
                    Field(Regular("b".to_owned())),
                ],
                true.into(),
                map![
                    "foo":
                        vec![
                            Value::Null,
                            Value::Null,
                            map!["bar baz": map!["a": map!["b": true]],].into()
                        ]
                ],
                Ok(()),
            ),
            (
                map!["foo": vec![0, 1, 2]],
                vec![Field(Regular("foo".to_owned())), Index(5)],
                "baz".into(),
                map![
                    "foo":
                        vec![
                            0.into(),
                            1.into(),
                            2.into(),
                            Value::Null,
                            Value::Null,
                            Value::from("baz"),
                        ]
                ],
                Ok(()),
            ),
            (
                map!["foo": "bar"],
                vec![Field(Regular("foo".to_owned())), Index(0)],
                "baz".into(),
                map!["foo": vec!["baz"]],
                Ok(()),
            ),
            (
                map!["foo": Value::Array(vec![])],
                vec![Field(Regular("foo".to_owned())), Index(0)],
                "baz".into(),
                map!["foo": vec!["baz"]],
                Ok(()),
            ),
            (
                map!["foo": Value::Array(vec![0.into()])],
                vec![Field(Regular("foo".to_owned())), Index(0)],
                "baz".into(),
                map!["foo": vec!["baz"]],
                Ok(()),
            ),
            (
                map!["foo": Value::Array(vec![0.into(), 1.into()])],
                vec![Field(Regular("foo".to_owned())), Index(0)],
                "baz".into(),
                map!["foo": Value::Array(vec!["baz".into(), 1.into()])],
                Ok(()),
            ),
            (
                map!["foo": Value::Array(vec![0.into(), 1.into()])],
                vec![Field(Regular("foo".to_owned())), Index(1)],
                "baz".into(),
                map!["foo": Value::Array(vec![0.into(), "baz".into()])],
                Ok(()),
            ),
        ];

        for (object, segments, value, expect, result) in cases {
            let object: BTreeMap<String, Value> = object;
            let mut event = Event::from(object);
            let expect = Event::from(expect);
            let value: remap::Value = value;
            let path = Path::new_unchecked(segments);

            assert_eq!(event.insert(&path, value.clone()), result);
            assert_eq!(event, expect);
            assert_eq!(event.get(&path), Ok(Some(value)));
        }
    }

    #[test]
    fn object_remove() {
        use remap::{map, Field::*, Object, Path, Segment::*};

        let cases = vec![
            (
                map!["foo": "bar"],
                vec![Field(Regular("foo".to_owned()))],
                false,
                Some(map![].into()),
            ),
            (
                map!["foo": "bar"],
                vec![Coalesce(vec![
                    Quoted("foo bar".to_owned()),
                    Regular("foo".to_owned()),
                ])],
                false,
                Some(map![].into()),
            ),
            (
                map!["foo": "bar", "baz": "qux"],
                vec![],
                false,
                Some(map![].into()),
            ),
            (
                map!["foo": "bar", "baz": "qux"],
                vec![],
                true,
                Some(map![].into()),
            ),
            (
                map!["foo": vec![0]],
                vec![Field(Regular("foo".to_owned())), Index(0)],
                false,
                Some(map!["foo": Value::Array(vec![])].into()),
            ),
            (
                map!["foo": vec![0]],
                vec![Field(Regular("foo".to_owned())), Index(0)],
                true,
                Some(map![].into()),
            ),
            (
                map!["foo": map!["bar baz": vec![0]], "bar": "baz"],
                vec![
                    Field(Regular("foo".to_owned())),
                    Field(Quoted("bar baz".to_owned())),
                    Index(0),
                ],
                false,
                Some(map!["foo": map!["bar baz": Value::Array(vec![])], "bar": "baz"].into()),
            ),
            (
                map!["foo": map!["bar baz": vec![0]], "bar": "baz"],
                vec![
                    Field(Regular("foo".to_owned())),
                    Field(Quoted("bar baz".to_owned())),
                    Index(0),
                ],
                true,
                Some(map!["bar": "baz"].into()),
            ),
        ];

        for (object, segments, compact, expect) in cases {
            let object: BTreeMap<String, Value> = object;
            let mut event = Event::from(object);
            let path = Path::new_unchecked(segments);

            assert_eq!(event.remove(&path, compact), Ok(()));
            assert_eq!(event.get(&Path::root()), Ok(expect))
        }
    }

    #[test]
    fn object_paths() {
        use remap::{map, Object, Path};
        use std::str::FromStr;

        let cases = vec![
            (map![], Ok(vec!["."])),
            (map!["foo bar baz": "bar"], Ok(vec![r#"."foo bar baz""#])),
            (map!["foo": "bar", "baz": "qux"], Ok(vec![".baz", ".foo"])),
            (map!["foo": map!["bar": "baz"]], Ok(vec![".foo.bar"])),
            (map!["a": vec![0, 1]], Ok(vec![".a[0]", ".a[1]"])),
            (
                map!["a": map!["b": "c"], "d": 12, "e": vec![
                    map!["f": 1],
                    map!["g": 2],
                    map!["h": 3],
                ]],
                Ok(vec![".a.b", ".d", ".e[0].f", ".e[1].g", ".e[2].h"]),
            ),
            (
                map![
                    "a": vec![map![
                        "b": vec![map!["c": map!["d": map!["e": vec![vec![0, 1]]]]]]
                    ]]
                ],
                Ok(vec![".a[0].b[0].c.d.e[0][0]", ".a[0].b[0].c.d.e[0][1]"]),
            ),
        ];

        for (object, expect) in cases {
            let object: BTreeMap<String, Value> = object;
            let event = Event::from(object);

            assert_eq!(
                event.paths(),
                expect.map(|vec| vec.iter().map(|s| Path::from_str(s).unwrap()).collect())
            );
        }
    }
}
