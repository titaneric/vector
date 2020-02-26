use crate::event::Event;
use crate::sources::Source;
use bytes::Bytes;
use codec::BytesDelimitedCodec;
use futures01::{future, sync::mpsc, Future, Sink, Stream};
use serde::{Deserialize, Serialize};
use std::{io, net::SocketAddr};
use string_cache::DefaultAtom as Atom;
use tokio::net::udp::{UdpFramed, UdpSocket};

/// UDP processes messages per packet, where messages are separated by newline.
#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct UdpConfig {
    pub address: SocketAddr,
    pub host_key: Option<Atom>,
}

impl UdpConfig {
    pub fn new(address: SocketAddr) -> Self {
        Self {
            address,
            host_key: None,
        }
    }
}

pub fn udp(address: SocketAddr, host_key: Atom, out: mpsc::Sender<Event>) -> Source {
    let out = out.sink_map_err(|e| error!("error sending event: {:?}", e));

    Box::new(
        future::lazy(move || {
            let socket = UdpSocket::bind(&address).expect("failed to bind to udp listener socket");

            info!(message = "listening.", %address);

            Ok(socket)
        })
        .and_then(move |socket| {
            let host_key = host_key.clone();
            // UDP processes messages per packet, where messages are separated by newline.
            // And stretch to end of packet.
            UdpFramed::with_decode(socket, BytesDelimitedCodec::new(b'\n'), true)
                .map(move |(line, addr): (Bytes, _)| {
                    let mut event = Event::from(line);

                    event
                        .as_mut_log()
                        .insert(host_key.clone(), addr.to_string());

                    trace!(message = "Received one event.", ?event);
                    event
                })
                // Error from Decoder or UdpSocket
                .map_err(|error: io::Error| error!(message = "error reading datagram.", %error))
                .forward(out)
                // Done with listening and sending
                .map(|_| ())
        }),
    )
}
