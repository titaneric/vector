use bytes::Bytes;
use chrono::{DateTime, Datelike, Utc};
use lookup::event_path;
use serde::{Deserialize, Serialize};
use smallvec::{smallvec, SmallVec};
use std::collections::BTreeMap;
use syslog_loose::{IncompleteDate, Message, ProcId, Protocol};
use value::{kind::Collection, Kind};
use vector_core::config::LogNamespace;
use vector_core::{
    config::{log_schema, DataType},
    event::{Event, LogEvent, Value},
    schema,
};

use super::Deserializer;

/// Config used to build a `SyslogDeserializer`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct SyslogDeserializerConfig;

impl SyslogDeserializerConfig {
    /// Build the `SyslogDeserializer` from this configuration.
    pub const fn build(&self) -> SyslogDeserializer {
        SyslogDeserializer
    }

    /// Return the type of event build by this deserializer.
    pub fn output_type(&self) -> DataType {
        DataType::Log
    }

    /// The schema produced by the deserializer.
    pub fn schema_definition(&self, log_namespace: LogNamespace) -> schema::Definition {
        match log_namespace {
            LogNamespace::Legacy => {
                schema::Definition::empty_legacy_namespace()
                    // The `message` field is always defined. If parsing fails, the entire body becomes the
                    // message.
                    .with_field(log_schema().message_key(), Kind::bytes(), Some("message"))
                    // All other fields are optional.
                    .optional_field(
                        log_schema().timestamp_key(),
                        Kind::timestamp(),
                        Some("timestamp"),
                    )
                    .optional_field("hostname", Kind::bytes(), None)
                    .optional_field("severity", Kind::bytes(), Some("severity"))
                    .optional_field("facility", Kind::bytes(), None)
                    .optional_field("version", Kind::integer(), None)
                    .optional_field("appname", Kind::bytes(), None)
                    .optional_field("msgid", Kind::bytes(), None)
                    .optional_field("procid", Kind::integer().or_bytes(), None)
                    // "structured data" is placed at the root. It will always be a map of strings
                    .unknown_fields(Kind::object(Collection::from_unknown(Kind::bytes())))
            }
            LogNamespace::Vector => {
                schema::Definition::new_with_default_metadata(
                    Kind::object(Collection::empty()),
                    [log_namespace],
                )
                .with_field("message", Kind::bytes(), Some("message"))
                .optional_field("timestamp", Kind::timestamp(), Some("timestamp"))
                .optional_field("hostname", Kind::bytes(), None)
                .optional_field("severity", Kind::bytes(), Some("severity"))
                .optional_field("facility", Kind::bytes(), None)
                .optional_field("version", Kind::integer(), None)
                .optional_field("appname", Kind::bytes(), None)
                .optional_field("msgid", Kind::bytes(), None)
                .optional_field("procid", Kind::integer().or_bytes(), None)
                // "structured data" is placed at the root. It will always be a map strings
                .unknown_fields(Kind::object(Collection::from_unknown(Kind::bytes())))
            }
        }
    }
}

/// Deserializer that builds an `Event` from a byte frame containing a syslog
/// message.
#[derive(Debug, Clone)]
pub struct SyslogDeserializer;

impl Deserializer for SyslogDeserializer {
    fn parse(
        &self,
        bytes: Bytes,
        log_namespace: LogNamespace,
    ) -> vector_common::Result<SmallVec<[Event; 1]>> {
        let line = std::str::from_utf8(&bytes)?;
        let line = line.trim();
        let parsed = syslog_loose::parse_message_with_year_exact(line, resolve_year)?;

        let mut log = LogEvent::from(Value::Object(BTreeMap::new()));
        insert_fields_from_syslog(&mut log, parsed, log_namespace);

        Ok(smallvec![Event::from(log)])
    }
}

/// Function used to resolve the year for syslog messages that don't include the
/// year.
///
/// If the current month is January, and the syslog message is for December, it
/// will take the previous year.
///
/// Otherwise, take the current year.
fn resolve_year((month, _date, _hour, _min, _sec): IncompleteDate) -> i32 {
    let now = Utc::now();
    if now.month() == 1 && month == 12 {
        now.year() - 1
    } else {
        now.year()
    }
}

fn insert_fields_from_syslog(
    log: &mut LogEvent,
    parsed: Message<&str>,
    log_namespace: LogNamespace,
) {
    match log_namespace {
        LogNamespace::Legacy => {
            log.insert(event_path!(log_schema().message_key()), parsed.msg);
        }
        LogNamespace::Vector => {
            log.insert(event_path!("message"), parsed.msg);
        }
    }

    if let Some(timestamp) = parsed.timestamp {
        let timestamp = DateTime::<Utc>::from(timestamp);
        match log_namespace {
            LogNamespace::Legacy => {
                log.insert(event_path!(log_schema().timestamp_key()), timestamp);
            }
            LogNamespace::Vector => {
                log.insert(event_path!("timestamp"), timestamp);
            }
        };
    }
    if let Some(host) = parsed.hostname {
        log.insert(event_path!("hostname"), host.to_string());
    }
    if let Some(severity) = parsed.severity {
        log.insert(event_path!("severity"), severity.as_str().to_owned());
    }
    if let Some(facility) = parsed.facility {
        log.insert(event_path!("facility"), facility.as_str().to_owned());
    }
    if let Protocol::RFC5424(version) = parsed.protocol {
        log.insert(event_path!("version"), version as i64);
    }
    if let Some(app_name) = parsed.appname {
        log.insert(event_path!("appname"), app_name.to_owned());
    }
    if let Some(msg_id) = parsed.msgid {
        log.insert(event_path!("msgid"), msg_id.to_owned());
    }
    if let Some(procid) = parsed.procid {
        let value: Value = match procid {
            ProcId::PID(pid) => pid.into(),
            ProcId::Name(name) => name.to_string().into(),
        };
        log.insert(event_path!("procid"), value);
    }

    for element in parsed.structured_data.into_iter() {
        let mut sdata: BTreeMap<String, Value> = BTreeMap::new();
        for (name, value) in element.params() {
            sdata.insert(name.to_string(), value.into());
        }
        log.insert(event_path!(element.id), sdata);
    }
}

#[cfg(test)]
mod tests {
    use vector_core::config::{init_log_schema, log_schema, LogSchema};

    use super::*;

    #[test]
    fn deserialize_syslog_legacy_namespace() {
        init_log_schema(
            || {
                let mut schema = LogSchema::default();
                schema.set_message_key("legacy_message".to_string());
                schema.set_message_key("legacy_timestamp".to_string());
                Ok(schema)
            },
            false,
        )
        .unwrap();

        let input =
            Bytes::from("<34>1 2003-10-11T22:14:15.003Z mymachine.example.com su - ID47 - MSG");
        let deserializer = SyslogDeserializer;

        let events = deserializer.parse(input, LogNamespace::Legacy).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].as_log()[log_schema().message_key()], "MSG".into());
        assert!(events[0].as_log()[log_schema().timestamp_key()].is_timestamp());
    }

    #[test]
    fn deserialize_syslog_vector_namespace() {
        init_log_schema(
            || {
                let mut schema = LogSchema::default();
                schema.set_message_key("legacy_message".to_string());
                schema.set_message_key("legacy_timestamp".to_string());
                Ok(schema)
            },
            false,
        )
        .unwrap();

        let input =
            Bytes::from("<34>1 2003-10-11T22:14:15.003Z mymachine.example.com su - ID47 - MSG");
        let deserializer = SyslogDeserializer;

        let events = deserializer.parse(input, LogNamespace::Vector).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].as_log()["message"], "MSG".into());
        assert!(events[0].as_log()["timestamp"].is_timestamp());
    }
}
