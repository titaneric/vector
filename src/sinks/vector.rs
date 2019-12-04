use crate::{
    buffers::Acker,
    event::proto,
    sinks::tcp::TcpSink,
    sinks::util::SinkExt,
    topology::config::{DataType, SinkConfig, SinkContext, SinkDescription},
    Event,
};
use bytes::{BufMut, Bytes, BytesMut};
use futures::{future, Future, Sink};
use prost::Message;
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use std::net::{SocketAddr, ToSocketAddrs};
use tokio::net::TcpStream;

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct VectorSinkConfig {
    pub address: String,
}

impl VectorSinkConfig {
    pub fn new(address: String) -> Self {
        Self { address }
    }
}

inventory::submit! {
    SinkDescription::new_without_default::<VectorSinkConfig>("vector")
}

#[typetag::serde(name = "vector")]
impl SinkConfig for VectorSinkConfig {
    fn build(&self, cx: SinkContext) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
        let addr = self
            .address
            .to_socket_addrs()
            .context(super::SocketAddressError)?
            .next()
            .ok_or(Box::new(super::BuildError::DNSFailure {
                address: self.address.clone(),
            }))?;

        let sink = vector(self.address.clone(), addr, cx.acker());
        let healthcheck = super::tcp::tcp_healthcheck(addr);

        Ok((sink, healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "vector"
    }
}

pub fn vector(hostname: String, addr: SocketAddr, acker: Acker) -> super::RouterSink {
    Box::new(
        TcpSink::new(hostname, addr, None)
            .stream_ack(acker)
            .with(encode_event),
    )
}

#[derive(Debug, Snafu)]
enum HealthcheckError {
    #[snafu(display("Connect error: {}", source))]
    ConnectError { source: std::io::Error },
}

pub fn vector_healthcheck(addr: SocketAddr) -> super::Healthcheck {
    // Lazy to avoid immediately connecting
    let check = future::lazy(move || {
        TcpStream::connect(&addr)
            .map(|_| ())
            .map_err(|err| err.into())
    });

    Box::new(check)
}

fn encode_event(event: Event) -> Result<Bytes, ()> {
    let event = proto::EventWrapper::from(event);
    let event_len = event.encoded_len() as u32;
    let full_len = event_len + 4;

    let mut out = BytesMut::with_capacity(full_len as usize);
    out.put_u32_be(event_len);
    event.encode(&mut out).unwrap();
    Ok(out.freeze())
}
