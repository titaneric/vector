use chrono::prelude::*;
use lazy_static::lazy_static;
use regex::{Captures, Regex};
use std::collections::BTreeMap;
use vrl::prelude::*;

lazy_static! {
    // Information about the common log format taken from the
    // - W3C specification: https://www.w3.org/Daemon/User/Config/Logging.html#common-logfile-format
    // - Apache HTTP Server docs: https://httpd.apache.org/docs/1.3/logs.html#common
    pub static ref REGEX_APACHE_COMMON_LOG: Regex = Regex::new(
        r#"(?x)                                 # Ignore whitespace and comments in the regex expression.
        ^\s*                                    # Start with any number of whitespaces.
        (-|(?P<host>.*?))\s+                    # Match `-` or any character (non-greedily) and at least one whitespace.
        (-|(?P<identity>.*?))\s+                # Match `-` or any character (non-greedily) and at least one whitespace.
        (-|(?P<user>.*?))\s+                    # Match `-` or any character (non-greedily) and at least one whitespace.
        (-|\[(-|(?P<timestamp>[^\[]*))\])\s+    # Match `-` or `[` followed by `-` or any character except `]`, `]` and at least one whitespace.
        (-|"(-|(\s*                             # Match `-` or `"` followed by `-` or and any number of whitespaces...
        (?P<message>(                           # Match a request with...
        (?P<method>\w+)\s+                      # Match at least one word character and at least one whitespace.
        (?P<path>[[\\"][^"]]*?)\s+              # Match any character except `"`, but `\"` (non-greedily) and at least one whitespace.
        (?P<protocol>[[\\"][^"]]*?)\s*          # Match any character except `"`, but `\"` (non-greedily) and any number of whitespaces.
        |[[\\"][^"]]*?))\s*))"                  # ...Or match any charater except `"`, but `\"`, and any amount of whitespaces.
        )\s+                                    # Match at least one whitespace.
        (-|(?P<status>\d+))\s+                  # Match `-` or at least one digit and at least one whitespace.
        (-|(?P<size>\d+))                       # Match `-` or at least one digit.
        \s*$                                    # Match any number of whitespaces (to be discarded).
    "#)
    .expect("failed compiling regex for common log");

    // - Apache HTTP Server docs: https://httpd.apache.org/docs/1.3/logs.html#combined
    pub static ref REGEX_APACHE_COMBINED_LOG: Regex = Regex::new(
        r#"(?x)                                 # Ignore whitespace and comments in the regex expression.
        ^\s*                                    # Start with any number of whitespaces.
        (-|(?P<host>.*?))\s+                    # Match `-` or any character (non-greedily) and at least one whitespace.
        (-|(?P<identity>.*?))\s+                # Match `-` or any character (non-greedily) and at least one whitespace.
        (-|(?P<user>.*?))\s+                    # Match `-` or any character (non-greedily) and at least one whitespace.
        (-|\[(-|(?P<timestamp>[^\[]*))\])\s+    # Match `-` or `[` followed by `-` or any character except `]`, `]` and at least one whitespace.
        (-|"(-|(\s*                             # Match `-` or `"` followed by `-` or and any number of whitespaces...
        (?P<message>(                           # Match a request with...
        (?P<method>\w+)\s+                      # Match at least one word character and at least one whitespace.
        (?P<path>[[\\"][^"]]*?)\s+              # Match any character except `"`, but `\"` (non-greedily) and at least one whitespace.
        (?P<protocol>[[\\"][^"]]*?)\s*          # Match any character except `"`, but `\"` (non-greedily) and any number of whitespaces.
        |[[\\"][^"]]*?))\s*))"                  # ...Or match any charater except `"`, but `\"`, and any amount of whitespaces.
        )\s+                                    # Match at least one whitespace.
        (-|(?P<status>\d+))\s+                  # Match `-` or at least one digit and at least one whitespace.
        (-|(?P<size>\d+))\s+                    # Match `-` or at least one digit.
        (-|"(-|(\s*                             # Match `-` or `"` followed by `-` or and any number of whitespaces...
        (?P<referrer>[[\\"][^"]]*?)             # Match any character except `"`, but `\"`
        ")))                                    # Match the closing quote
        \s+                                     # Match whitespace
        (-|"(-|(\s*                             # Match `-` or `"` followed by `-` or and any number of whitespaces...
        (?P<agent>[[\\"][^"]]*?)                # Match any character except `"`, but `\"`
        ")))                                    # Match the closing quote
        #\s*$                                   # Match any number of whitespaces (to be discarded).
    "#)
    .expect("failed compiling regex for combined log");

    // It is possible to customise the format output by apache. This function just handles the default defined here.
    // https://github.com/mingrammer/flog/blob/9bc83b14408ca446e934c32e4a88a81a46e78d83/log.go#L16
    pub static ref REGEX_APACHE_ERROR_LOG: Regex = Regex::new(
        r#"(?x)                                     # Ignore whitespace and comments in the regex expression.
        ^\s*                                        # Start with any number of whitespaces.
        (-|\[(-|(?P<timestamp>[^\[]*))\])\s+        # Match `-` or `[` followed by `-` or any character except `]`, `]` and at least one whitespace.
        (-|\[(-|(?P<module>[^:]*):                  # Match `-` or `[` followed by `-` or any character except `:`.
        (?P<severity>[^\[]*))\])\s+                 # Match ary character except `]`, `]` and at least one whitespace.
        (-|\[\s*pid\s*(-|(?P<pid>[^:]*):            # Match `-` or `[` followed by `pid`, `-` or any character except `:`.
        \s*tid\s*(?P<thread>[^\[]*))\])\s           # Match `tid` followed by any character except `]`, `]` and at least one whitespace.
        (-|\[\s*client\s*(-|(?P<client>[^:]*):      # Match `-` or `[` followed by `client`, `-` or any character except `:`.
        (?P<port>[^\[]*))\])\s                      # Match `-` or `[` followed by `-` or any character except `]`, `]` and at least one whitespace.
        (-|(?P<message>.*))                         # Match `-` or any character.
        \s*$                                        # Match any number of whitespaces (to be discarded).
    "#)
    .expect("failed compiling regex for error log");

}

// Parse the time as Utc if we can extract the timezone.
// If we can't `chrono` will error, and we will have to parse the time as a local time.
fn parse_time(time: &str, format: &str) -> std::result::Result<DateTime<Utc>, String> {
    DateTime::parse_from_str(time, &format)
        .map(Into::into)
        .or_else(|_| {
            let parsed =
                &chrono::NaiveDateTime::parse_from_str(time, &format).map_err(|error| {
                    format!(
                        r#"failed parsing timestamp {} using format {}: {}"#,
                        time, format, error
                    )
                })?;

            let result = Local.from_local_datetime(&parsed).earliest();

            match result {
                Some(result) => Ok(result.into()),
                None => Ok(Local.from_utc_datetime(parsed).into()),
            }
        })
}

/// Takes the field as a string and returns a `Value`.
/// Most fields are `Value::Bytes`, but some are other types, we convert to those
/// types based on the fieldname.
fn capture_value(
    name: &str,
    value: &str,
    timestamp_format: &str,
) -> std::result::Result<Value, String> {
    Ok(match name {
        "timestamp" => Value::Timestamp(parse_time(&value, &timestamp_format)?),
        "status" | "size" | "pid" | "port" => Value::Integer(
            value
                .parse()
                .map_err(|_| format!("failed parsing {}", name))?,
        ),
        _ => Value::Bytes(value.to_owned().into()),
    })
}

/// Extracts the log fields from the regex and adds them to a `Value::Object`.
pub fn log_fields(
    regex: &Regex,
    captures: &Captures,
    timestamp_format: &str,
) -> std::result::Result<Value, String> {
    Ok(regex
        .capture_names()
        .filter_map(|name| {
            name.and_then(|name| {
                captures.name(name).map(|value| {
                    Ok((
                        name.to_string(),
                        capture_value(&name, &value.as_str(), &timestamp_format)?,
                    ))
                })
            })
        })
        .collect::<std::result::Result<BTreeMap<String, Value>, String>>()?
        .into())
}
