use chrono::{DateTime, Utc};
use derivative::Derivative;
use vector_lib::{
    config::{LegacyKey, LogNamespace, log_schema},
    conversion,
    lookup::path,
};

use crate::{
    event::{Event, Value},
    internal_events::{
        DROP_EVENT, ParserConversionError, ParserMatchError, ParserMissingFieldError,
    },
    sources::kubernetes_logs::{Config, transform_utils::get_message_path},
    transforms::{FunctionTransform, OutputBuffer},
};

const TIMESTAMP_KEY: &str = "timestamp";

/// Parser for the API log format.
///
/// Expects logs to arrive in an API log format.
///
/// API log format is a simple newline-separated text format. We rely on regular expressions to parse it.
///
/// Normalizes parsed data for consistency.
///
#[derive(Clone, Derivative)]
#[derivative(Debug)]
pub(super) struct Api {
    log_namespace: LogNamespace,
}

impl Api {
    pub const fn new(log_namespace: LogNamespace) -> Self {
        Self { log_namespace }
    }
}

impl FunctionTransform for Api {
    fn transform(&mut self, output: &mut OutputBuffer, mut event: Event) {
        let message_path = get_message_path(self.log_namespace);

        // Get the log field with the message, if it exists, and coerce it to bytes.
        let log = event.as_mut_log();
        let value = log.remove(&message_path).map(|s| s.coerce_to_bytes());
        match value {
            None => {
                // The message field was missing, inexplicably. If we can't find the message field, there's nothing for
                // us to actually decode, so there's no event we could emit, and so we just emit the error and return.
                emit!(ParserMissingFieldError::<DROP_EVENT> {
                    field: &message_path.to_string()
                });
                return;
            }
            Some(s) => match parse_log_line(&s) {
                None => {
                    emit!(ParserMatchError { value: &s[..] });
                    return;
                }
                Some(parsed_log) => {
                    // For all fields except `timestamp`, simply treat them as `Value::Bytes`. For
                    // `timestamp`, however, we actually make sure we can convert it correctly and feed it
                    // in as `Value::Timestamp`.

                    // MESSAGE
                    // Insert either directly into `.` or `log_schema().message_key()`,
                    // overwriting the original "full" CRI log that included additional fields.
                    drop(log.insert(&message_path, Value::Bytes(s.slice_ref(parsed_log.message))));

                    // TIMESTAMP_TAG
                    let ds = String::from_utf8_lossy(parsed_log.timestamp);
                    match DateTime::parse_from_str(&ds, "%+") {
                        Ok(dt) =>
                        // Insert the TIMESTAMP_TAG parsed out of the CRI log, this is the timestamp of
                        // when the runtime processed this message.
                        {
                            self.log_namespace.insert_source_metadata(
                                Config::NAME,
                                log,
                                log_schema().timestamp_key().map(LegacyKey::Overwrite),
                                path!(TIMESTAMP_KEY),
                                Value::Timestamp(dt.with_timezone(&Utc)),
                            )
                        }
                        Err(e) => {
                            emit!(ParserConversionError {
                                name: TIMESTAMP_KEY,
                                error: conversion::Error::TimestampParse {
                                    s: ds.to_string(),
                                    source: e,
                                },
                            });
                        }
                    }
                }
            },
        }

        output.push(event);
    }
}

struct ParsedLog<'a> {
    timestamp: &'a [u8],
    message: &'a [u8],
}

#[allow(clippy::trivially_copy_pass_by_ref)]
#[inline]
const fn is_delimiter(c: &u8) -> bool {
    *c == b' '
}

/// Parses a CRI log line.
///
/// Equivalent to regex: `(?-u)^(?P<timestamp>.*) (?P<stream>(stdout|stderr)) (?P<multiline_tag>(P|F)) (?P<message>.*)(?P<new_line_tag>\n?)$`
#[inline]
fn parse_log_line(line: &[u8]) -> Option<ParsedLog<'_>> {
    let rest = line;

    let after_timestamp_pos = rest.iter().position(is_delimiter)?;
    let (timestamp, rest) = rest.split_at(after_timestamp_pos + 1);
    let timestamp = timestamp.split_last()?.1; // Trim the delimiter

    let has_new_line_tag = !rest.is_empty() && *rest.last()? == b'\n';
    let message = if has_new_line_tag {
        // Remove the newline tag field, if it exists.
        // For additional details, see https://github.com/vectordotdev/vector/issues/8606.
        rest.split_last()?.1
    } else {
        rest
    };

    Some(ParsedLog { timestamp, message })
}

#[cfg(test)]
pub mod tests {
    use bytes::Bytes;
    use vrl::value;

    use super::{super::test_util, *};
    use crate::{event::LogEvent, test_util::trace_init};

    /// Shared test cases.
    pub fn valid_cases(log_namespace: LogNamespace) -> Vec<(Bytes, Vec<Event>)> {
        vec![
            (
                Bytes::from("2016-10-06T00:17:09.669794202Z The content of the log entry 1"),
                vec![test_util::make_api_log_event(
                    value!("The content of the log entry 1"),
                    "2016-10-06T00:17:09.669794202Z",
                    log_namespace,
                )],
            ),
            (
                Bytes::from("2016-10-06T00:17:09.669794202Z First line of log entry 2"),
                vec![test_util::make_api_log_event(
                    value!("First line of log entry 2"),
                    "2016-10-06T00:17:09.669794202Z",
                    log_namespace,
                )],
            ),
            (
                Bytes::from("2016-10-06T00:17:09.669794202Z Second line of the log entry 2"),
                vec![test_util::make_api_log_event(
                    value!("Second line of the log entry 2"),
                    "2016-10-06T00:17:09.669794202Z",
                    log_namespace,
                )],
            ),
            (
                Bytes::from("2016-10-06T00:17:10.113242941Z Last line of the log entry 2"),
                vec![test_util::make_api_log_event(
                    value!("Last line of the log entry 2"),
                    "2016-10-06T00:17:10.113242941Z",
                    log_namespace,
                )],
            ),
            (
                // This is not valid UTF-8 string, ends with \n
                // 2021-08-05T17:35:26.640507539Z Hello World Привет Ми\xd1\n
                Bytes::from(vec![
                    50, 48, 50, 49, 45, 48, 56, 45, 48, 53, 84, 49, 55, 58, 51, 53, 58, 50, 54, 46,
                    54, 52, 48, 53, 48, 55, 53, 51, 57, 90, 32, 72, 101, 108, 108, 111, 32, 87,
                    111, 114, 108, 100, 32, 208, 159, 209, 128, 208, 184, 208, 178, 208, 181, 209,
                    130, 32, 208, 156, 208, 184, 209, 10,
                ]),
                vec![test_util::make_api_log_event(
                    value!(Bytes::from(vec![
                        72, 101, 108, 108, 111, 32, 87, 111, 114, 108, 100, 32, 208, 159, 209, 128,
                        208, 184, 208, 178, 208, 181, 209, 130, 32, 208, 156, 208, 184, 209,
                    ])),
                    "2021-08-05T17:35:26.640507539Z",
                    log_namespace,
                )],
            ),
        ]
    }

    #[test]
    fn test_parsing_valid_vector_namespace() {
        trace_init();
        test_util::test_parser(
            || Api::new(LogNamespace::Vector),
            |bytes| Event::Log(LogEvent::from(value!(bytes))),
            valid_cases(LogNamespace::Vector),
        );
    }

    #[test]
    fn test_parsing_valid_legacy_namespace() {
        trace_init();
        test_util::test_parser(
            || Api::new(LogNamespace::Legacy),
            |bytes| Event::Log(LogEvent::from(bytes)),
            valid_cases(LogNamespace::Legacy),
        );
    }
}
