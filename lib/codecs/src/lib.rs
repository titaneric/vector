//! A collection of codecs that can be used to transform between bytes streams /
//! byte messages, byte frames and structured events.

#![deny(missing_docs)]
#![deny(warnings)]

pub mod decoding;
pub mod encoding;
pub mod gelf;

pub use decoding::{
    BytesDecoder, BytesDecoderConfig, BytesDeserializer, BytesDeserializerConfig,
    CharacterDelimitedDecoder, CharacterDelimitedDecoderConfig, GelfDeserializer,
    GelfDeserializerConfig, JsonDeserializer, JsonDeserializerConfig, LengthDelimitedDecoder,
    LengthDelimitedDecoderConfig, NativeDeserializer, NativeDeserializerConfig,
    NativeJsonDeserializer, NativeJsonDeserializerConfig, NewlineDelimitedDecoder,
    NewlineDelimitedDecoderConfig, OctetCountingDecoder, OctetCountingDecoderConfig,
    StreamDecodingError,
};
#[cfg(feature = "syslog")]
pub use decoding::{SyslogDeserializer, SyslogDeserializerConfig};
pub use encoding::{
    BytesEncoder, BytesEncoderConfig, CharacterDelimitedEncoder, CharacterDelimitedEncoderConfig,
    JsonSerializer, JsonSerializerConfig, LengthDelimitedEncoder, LengthDelimitedEncoderConfig,
    LogfmtSerializer, LogfmtSerializerConfig, NativeJsonSerializer, NativeJsonSerializerConfig,
    NativeSerializer, NativeSerializerConfig, NewlineDelimitedEncoder,
    NewlineDelimitedEncoderConfig, RawMessageSerializer, RawMessageSerializerConfig,
    TextSerializer, TextSerializerConfig,
};
pub use gelf::{gelf_fields, VALID_FIELD_REGEX};
