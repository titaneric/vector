use codecs::decoding::{DeserializerConfig, FramingConfig};
use serde::{Deserialize, Serialize};
use vector_core::config::LogNamespace;

use crate::codecs::Decoder;

/// Config used to build a `Decoder`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DecodingConfig {
    /// The framing config.
    framing: FramingConfig,
    /// The decoding config.
    decoding: DeserializerConfig,
    /// The namespace used when decoding.
    log_namespace: LogNamespace,
}

impl DecodingConfig {
    /// Creates a new `DecodingConfig` with the provided `FramingConfig` and
    /// `DeserializerConfig`.
    pub const fn new(
        framing: FramingConfig,
        decoding: DeserializerConfig,
        log_namespace: LogNamespace,
    ) -> Self {
        Self {
            framing,
            decoding,
            log_namespace,
        }
    }

    /// Builds a `Decoder` from the provided configuration.
    pub fn build(&self) -> Decoder {
        // Build the framer.
        let framer = self.framing.build();

        // Build the deserializer.
        let deserializer = self.decoding.build();

        Decoder::new(framer, deserializer).with_log_namespace(self.log_namespace)
    }
}
