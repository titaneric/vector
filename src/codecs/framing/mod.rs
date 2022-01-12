//! A collection of framing methods that can be used to convert between byte
//! chunks and byte frames with defined boundaries.

#![deny(missing_docs)]

mod bytes;
mod character_delimited;
mod length_delimited;
mod newline_delimited;
mod octet_counting;

pub use self::bytes::{BytesDecoder, BytesDecoderConfig};
pub use character_delimited::{
    CharacterDelimitedDecoder, CharacterDelimitedDecoderConfig, CharacterDelimitedEncoder,
    CharacterDelimitedEncoderConfig,
};
pub use length_delimited::{LengthDelimitedDecoder, LengthDelimitedDecoderConfig};
pub use newline_delimited::{
    NewlineDelimitedDecoder, NewlineDelimitedDecoderConfig, NewlineDelimitedEncoder,
    NewlineDelimitedEncoderConfig,
};
pub use octet_counting::{OctetCountingDecoder, OctetCountingDecoderConfig};
