use bytes::Bytes;
use serde::{Deserialize, Serialize};
use smallvec::{smallvec, SmallVec};
use value::Kind;

use super::Deserializer;
use crate::{
    config::log_schema,
    event::{Event, LogEvent},
    schema,
};

/// Config used to build a `BytesDeserializer`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct BytesDeserializerConfig;

impl BytesDeserializerConfig {
    /// Creates a new `BytesDeserializerConfig`.
    pub const fn new() -> Self {
        Self
    }

    /// Build the `BytesDeserializer` from this configuration.
    pub fn build(&self) -> BytesDeserializer {
        BytesDeserializer::new()
    }

    /// The schema produced by the deserializer.
    pub fn schema_definition(&self) -> schema::Definition {
        schema::Definition::empty().required_field(
            log_schema().message_key(),
            Kind::bytes(),
            Some("message"),
        )
    }
}

/// Deserializer that converts bytes to an `Event`.
///
/// This deserializer can be considered as the no-op action for input where no
/// further decoding has been specified.
#[derive(Debug, Clone)]
pub struct BytesDeserializer {
    log_schema_message_key: &'static str,
}

impl Default for BytesDeserializer {
    fn default() -> Self {
        Self::new()
    }
}

impl BytesDeserializer {
    /// Creates a new `BytesDeserializer`.
    pub fn new() -> Self {
        Self {
            log_schema_message_key: log_schema().message_key(),
        }
    }
}

impl Deserializer for BytesDeserializer {
    fn parse(&self, bytes: Bytes) -> crate::Result<SmallVec<[Event; 1]>> {
        let mut log = LogEvent::default();
        log.insert(self.log_schema_message_key, bytes);
        Ok(smallvec![log.into()])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::log_schema;

    #[test]
    fn deserialize_bytes() {
        let input = Bytes::from("foo");
        let deserializer = BytesDeserializer::new();

        let events = deserializer.parse(input).unwrap();
        let mut events = events.into_iter();

        {
            let event = events.next().unwrap();
            let log = event.as_log();
            assert_eq!(log[log_schema().message_key()], "foo".into());
        }

        assert_eq!(events.next(), None);
    }
}
