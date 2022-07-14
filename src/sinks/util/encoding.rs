use std::io;

use bytes::BytesMut;
use codecs::encoding::Framer;
use tokio_util::codec::Encoder as _;

use crate::{codecs::Transformer, event::Event};

pub trait Encoder<T> {
    /// Encodes the input into the provided writer.
    ///
    /// # Errors
    ///
    /// If an I/O error is encountered while encoding the input, an error variant will be returned.
    fn encode_input(&self, input: T, writer: &mut dyn io::Write) -> io::Result<usize>;
}

impl Encoder<Vec<Event>> for (Transformer, crate::codecs::Encoder<Framer>) {
    fn encode_input(
        &self,
        mut events: Vec<Event>,
        writer: &mut dyn io::Write,
    ) -> io::Result<usize> {
        let mut encoder = self.1.clone();
        let mut bytes_written = 0;
        let batch_prefix = encoder.batch_prefix();
        writer.write_all(batch_prefix)?;
        bytes_written += batch_prefix.len();
        if let Some(last) = events.pop() {
            for mut event in events {
                self.0.transform(&mut event);
                let mut bytes = BytesMut::new();
                encoder
                    .encode(event, &mut bytes)
                    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
                writer.write_all(&bytes)?;
                bytes_written += bytes.len();
            }
            let mut event = last;
            self.0.transform(&mut event);
            let mut bytes = BytesMut::new();
            encoder
                .serialize(event, &mut bytes)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
            writer.write_all(&bytes)?;
            bytes_written += bytes.len();
        }
        let batch_suffix = encoder.batch_suffix();
        writer.write_all(batch_suffix)?;
        bytes_written += batch_suffix.len();

        Ok(bytes_written)
    }
}

impl Encoder<Event> for (Transformer, crate::codecs::Encoder<()>) {
    fn encode_input(&self, mut event: Event, writer: &mut dyn io::Write) -> io::Result<usize> {
        let mut encoder = self.1.clone();
        self.0.transform(&mut event);
        let mut bytes = BytesMut::new();
        encoder
            .serialize(event, &mut bytes)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        writer.write_all(&bytes)?;
        Ok(bytes.len())
    }
}

pub fn as_tracked_write<F, I, E>(inner: &mut dyn io::Write, input: I, f: F) -> io::Result<usize>
where
    F: FnOnce(&mut dyn io::Write, I) -> Result<(), E>,
    E: Into<io::Error> + 'static,
{
    struct Tracked<'inner> {
        count: usize,
        inner: &'inner mut dyn io::Write,
    }

    impl<'inner> io::Write for Tracked<'inner> {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            #[allow(clippy::disallowed_methods)] // We pass on the result of `write` to the caller.
            let n = self.inner.write(buf)?;
            self.count += n;
            Ok(n)
        }

        fn flush(&mut self) -> io::Result<()> {
            self.inner.flush()
        }
    }

    let mut tracked = Tracked { count: 0, inner };
    f(&mut tracked, input).map_err(|e| e.into())?;
    Ok(tracked.count)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use codecs::{
        CharacterDelimitedEncoder, JsonSerializer, NewlineDelimitedEncoder, TextSerializer,
    };
    use value::Value;
    use vector_core::event::LogEvent;

    use super::*;

    #[test]
    fn test_encode_batch_json_empty() {
        let encoding = (
            Transformer::default(),
            crate::codecs::Encoder::<Framer>::new(
                CharacterDelimitedEncoder::new(b',').into(),
                JsonSerializer::new().into(),
            ),
        );

        let mut writer = Vec::new();
        let written = encoding.encode_input(vec![], &mut writer).unwrap();
        assert_eq!(written, 2);

        assert_eq!(String::from_utf8(writer).unwrap(), "[]");
    }

    #[test]
    fn test_encode_batch_json_single() {
        let encoding = (
            Transformer::default(),
            crate::codecs::Encoder::<Framer>::new(
                CharacterDelimitedEncoder::new(b',').into(),
                JsonSerializer::new().into(),
            ),
        );

        let mut writer = Vec::new();
        let written = encoding
            .encode_input(
                vec![Event::Log(LogEvent::from(BTreeMap::from([(
                    String::from("key"),
                    Value::from("value"),
                )])))],
                &mut writer,
            )
            .unwrap();
        assert_eq!(written, 17);

        assert_eq!(String::from_utf8(writer).unwrap(), r#"[{"key":"value"}]"#);
    }

    #[test]
    fn test_encode_batch_json_multiple() {
        let encoding = (
            Transformer::default(),
            crate::codecs::Encoder::<Framer>::new(
                CharacterDelimitedEncoder::new(b',').into(),
                JsonSerializer::new().into(),
            ),
        );

        let mut writer = Vec::new();
        let written = encoding
            .encode_input(
                vec![
                    Event::Log(LogEvent::from(BTreeMap::from([(
                        String::from("key"),
                        Value::from("value1"),
                    )]))),
                    Event::Log(LogEvent::from(BTreeMap::from([(
                        String::from("key"),
                        Value::from("value2"),
                    )]))),
                    Event::Log(LogEvent::from(BTreeMap::from([(
                        String::from("key"),
                        Value::from("value3"),
                    )]))),
                ],
                &mut writer,
            )
            .unwrap();
        assert_eq!(written, 52);

        assert_eq!(
            String::from_utf8(writer).unwrap(),
            r#"[{"key":"value1"},{"key":"value2"},{"key":"value3"}]"#
        );
    }

    #[test]
    fn test_encode_batch_ndjson_empty() {
        let encoding = (
            Transformer::default(),
            crate::codecs::Encoder::<Framer>::new(
                NewlineDelimitedEncoder::new().into(),
                JsonSerializer::new().into(),
            ),
        );

        let mut writer = Vec::new();
        let written = encoding.encode_input(vec![], &mut writer).unwrap();
        assert_eq!(written, 0);

        assert_eq!(String::from_utf8(writer).unwrap(), "");
    }

    #[test]
    fn test_encode_batch_ndjson_single() {
        let encoding = (
            Transformer::default(),
            crate::codecs::Encoder::<Framer>::new(
                NewlineDelimitedEncoder::new().into(),
                JsonSerializer::new().into(),
            ),
        );

        let mut writer = Vec::new();
        let written = encoding
            .encode_input(
                vec![Event::Log(LogEvent::from(BTreeMap::from([(
                    String::from("key"),
                    Value::from("value"),
                )])))],
                &mut writer,
            )
            .unwrap();
        assert_eq!(written, 15);

        assert_eq!(String::from_utf8(writer).unwrap(), r#"{"key":"value"}"#);
    }

    #[test]
    fn test_encode_batch_ndjson_multiple() {
        let encoding = (
            Transformer::default(),
            crate::codecs::Encoder::<Framer>::new(
                NewlineDelimitedEncoder::new().into(),
                JsonSerializer::new().into(),
            ),
        );

        let mut writer = Vec::new();
        let written = encoding
            .encode_input(
                vec![
                    Event::Log(LogEvent::from(BTreeMap::from([(
                        String::from("key"),
                        Value::from("value1"),
                    )]))),
                    Event::Log(LogEvent::from(BTreeMap::from([(
                        String::from("key"),
                        Value::from("value2"),
                    )]))),
                    Event::Log(LogEvent::from(BTreeMap::from([(
                        String::from("key"),
                        Value::from("value3"),
                    )]))),
                ],
                &mut writer,
            )
            .unwrap();
        assert_eq!(written, 50);

        assert_eq!(
            String::from_utf8(writer).unwrap(),
            "{\"key\":\"value1\"}\n{\"key\":\"value2\"}\n{\"key\":\"value3\"}"
        );
    }

    #[test]
    fn test_encode_event_json() {
        let encoding = (
            Transformer::default(),
            crate::codecs::Encoder::<()>::new(JsonSerializer::new().into()),
        );

        let mut writer = Vec::new();
        let written = encoding
            .encode_input(
                Event::Log(LogEvent::from(BTreeMap::from([(
                    String::from("key"),
                    Value::from("value"),
                )]))),
                &mut writer,
            )
            .unwrap();
        assert_eq!(written, 15);

        assert_eq!(String::from_utf8(writer).unwrap(), r#"{"key":"value"}"#);
    }

    #[test]
    fn test_encode_event_text() {
        let encoding = (
            Transformer::default(),
            crate::codecs::Encoder::<()>::new(TextSerializer::new().into()),
        );

        let mut writer = Vec::new();
        let written = encoding
            .encode_input(
                Event::Log(LogEvent::from(BTreeMap::from([(
                    String::from("message"),
                    Value::from("value"),
                )]))),
                &mut writer,
            )
            .unwrap();
        assert_eq!(written, 5);

        assert_eq!(String::from_utf8(writer).unwrap(), r#"value"#);
    }
}
