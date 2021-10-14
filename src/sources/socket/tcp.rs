use crate::{
    codecs::{self, FramingConfig, ParserConfig},
    event::Event,
    internal_events::{SocketEventsReceived, SocketMode},
    serde::{default_decoding, default_framing_stream_based},
    sources::util::{SocketListenAddr, TcpSource},
    tcp::TcpKeepaliveConfig,
    tls::TlsConfig,
};
use bytes::Bytes;
use getset::{CopyGetters, Getters, Setters};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

#[derive(Deserialize, Serialize, Debug, Clone, Getters, CopyGetters, Setters)]
pub struct TcpConfig {
    #[get_copy = "pub"]
    address: SocketListenAddr,
    #[get_copy = "pub"]
    keepalive: Option<TcpKeepaliveConfig>,
    #[serde(default = "default_shutdown_timeout_secs")]
    #[getset(get_copy = "pub", set = "pub")]
    shutdown_timeout_secs: u64,
    #[get = "pub"]
    host_key: Option<String>,
    #[getset(get = "pub", set = "pub")]
    tls: Option<TlsConfig>,
    #[get_copy = "pub"]
    receive_buffer_bytes: Option<usize>,
    #[serde(default = "default_framing_stream_based")]
    #[getset(get = "pub", set = "pub")]
    framing: Box<dyn FramingConfig>,
    #[serde(default = "default_decoding")]
    #[getset(get = "pub", set = "pub")]
    decoding: Box<dyn ParserConfig>,
}

const fn default_shutdown_timeout_secs() -> u64 {
    30
}

impl TcpConfig {
    pub fn new(
        address: SocketListenAddr,
        keepalive: Option<TcpKeepaliveConfig>,
        shutdown_timeout_secs: u64,
        host_key: Option<String>,
        tls: Option<TlsConfig>,
        receive_buffer_bytes: Option<usize>,
        framing: Box<dyn FramingConfig>,
        decoding: Box<dyn ParserConfig>,
    ) -> Self {
        Self {
            address,
            keepalive,
            shutdown_timeout_secs,
            host_key,
            tls,
            receive_buffer_bytes,
            framing,
            decoding,
        }
    }

    pub fn from_address(address: SocketListenAddr) -> Self {
        Self {
            address,
            keepalive: None,
            shutdown_timeout_secs: default_shutdown_timeout_secs(),
            host_key: None,
            tls: None,
            receive_buffer_bytes: None,
            framing: default_framing_stream_based(),
            decoding: default_decoding(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RawTcpSource {
    config: TcpConfig,
    decoder: codecs::Decoder,
}

impl RawTcpSource {
    pub const fn new(config: TcpConfig, decoder: codecs::Decoder) -> Self {
        Self { config, decoder }
    }
}

impl TcpSource for RawTcpSource {
    type Error = codecs::Error;
    type Item = SmallVec<[Event; 1]>;
    type Decoder = codecs::Decoder;

    fn decoder(&self) -> Self::Decoder {
        self.decoder.clone()
    }

    fn handle_events(&self, events: &mut [Event], host: Bytes, byte_size: usize) {
        emit!(&SocketEventsReceived {
            mode: SocketMode::Tcp,
            byte_size,
            count: events.len()
        });

        for event in events {
            if let Event::Log(ref mut log) = event {
                log.insert(
                    crate::config::log_schema().source_type_key(),
                    Bytes::from("socket"),
                );

                let host_key = (self.config.host_key.clone())
                    .unwrap_or_else(|| crate::config::log_schema().host_key().to_string());

                log.insert(host_key, host.clone());
            }
        }
    }
}
