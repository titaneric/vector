use crate::{
    config::{DataType, GenerateConfig, Resource, SinkContext},
    sinks::util::{tcp::TcpSinkConfig, EncodedEvent},
    sinks::{Healthcheck, VectorSink},
    tcp::TcpKeepaliveConfig,
    tls::TlsConfig,
};
use bytes::{BufMut, Bytes, BytesMut};
use getset::Setters;
use prost::Message;
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use vector_core::event::{proto, Event, WithMetadata};

#[derive(Deserialize, Serialize, Debug, Clone, Setters)]
#[serde(deny_unknown_fields)]
pub struct VectorConfig {
    address: String,
    keepalive: Option<TcpKeepaliveConfig>,
    #[set = "pub"]
    tls: Option<TlsConfig>,
    send_buffer_bytes: Option<usize>,
}

impl VectorConfig {
    pub fn new(
        address: String,
        keepalive: Option<TcpKeepaliveConfig>,
        tls: Option<TlsConfig>,
        send_buffer_bytes: Option<usize>,
    ) -> Self {
        Self {
            address,
            keepalive,
            tls,
            send_buffer_bytes,
        }
    }

    pub fn from_address(address: String) -> Self {
        Self::new(address, None, None, None)
    }
}

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("Missing host in address field"))]
    MissingHost,
    #[snafu(display("Missing port in address field"))]
    MissingPort,
}

impl GenerateConfig for VectorConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self::new("127.0.0.1:5000".to_string(), None, None, None)).unwrap()
    }
}

impl VectorConfig {
    pub(crate) async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let sink_config = TcpSinkConfig::new(
            self.address.clone(),
            self.keepalive,
            self.tls.clone(),
            self.send_buffer_bytes,
        );

        sink_config.build(cx, |event| Some(encode_event(event)))
    }

    pub(super) fn input_type(&self) -> DataType {
        DataType::Any
    }

    pub(super) fn sink_type(&self) -> &'static str {
        "vector"
    }

    pub(super) fn resources(&self) -> Vec<Resource> {
        Vec::new()
    }
}

#[derive(Debug, Snafu)]
enum HealthcheckError {
    #[snafu(display("Connect error: {}", source))]
    ConnectError { source: std::io::Error },
}

fn encode_event(event: Event) -> EncodedEvent<Bytes> {
    let event: WithMetadata<proto::EventWrapper> = event.into();
    let WithMetadata { data, metadata } = event;
    let event_len = data.encoded_len();
    let full_len = event_len + 4;

    let mut out = BytesMut::with_capacity(full_len);
    out.put_u32(event_len as u32);
    data.encode(&mut out).unwrap();

    EncodedEvent {
        item: out.into(),
        metadata: Some(metadata),
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<super::VectorConfig>();
    }
}
