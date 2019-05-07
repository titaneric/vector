use crate::Record;
use futures::{future, sync::mpsc, Future, Sink, Stream};
use parser::parse;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tokio::{
    self,
    codec::BytesCodec,
    net::{UdpFramed, UdpSocket},
};
use tokio_trace::field;

mod parser;

#[derive(Debug, PartialEq)]
pub enum Metric {
    Counter {
        name: String,
        val: usize,
        sampling: Option<f32>,
    },
    Timer {
        name: String,
        val: usize,
        sampling: Option<f32>,
    },
    Gauge {
        name: String,
        val: usize,
        direction: Option<Direction>,
    },
    Set {
        name: String,
        val: String,
    },
}

#[derive(Debug, PartialEq)]
pub enum Direction {
    Plus,
    Minus,
}

#[derive(Deserialize, Serialize, Debug)]
struct StatsdConfig {
    address: SocketAddr,
}

#[typetag::serde(name = "statsd")]
impl crate::topology::config::SourceConfig for StatsdConfig {
    fn build(&self, out: mpsc::Sender<Record>) -> Result<super::Source, String> {
        Ok(statsd(self.address.clone(), out))
    }
}

fn statsd(addr: SocketAddr, out: mpsc::Sender<Record>) -> super::Source {
    let out = out.sink_map_err(|e| error!("error sending metric: {:?}", e));

    Box::new(
        future::lazy(move || {
            let socket = UdpSocket::bind(&addr).expect("failed to bind to udp listener socket");

            info!(
                message = "listening.",
                addr = &field::display(addr),
                r#type = "udp"
            );

            future::ok(socket)
        })
        .and_then(|socket| {
            let metrics_in = UdpFramed::new(socket, BytesCodec::new())
                .map(|(bytes, _sock)| {
                    let packet = String::from_utf8_lossy(bytes.as_ref());
                    let metrics = packet
                        .lines()
                        .map(parse)
                        .filter_map(|res| res.map_err(|e| error!("{}", e)).ok())
                        .map(Record::from)
                        .collect::<Vec<_>>();
                    futures::stream::iter_ok::<_, std::io::Error>(metrics)
                })
                .flatten()
                .map_err(|e| error!("error reading datagram: {:?}", e));

            metrics_in.forward(out).map(|_| info!("finished sending"))
        }),
    )
}

impl From<Metric> for Record {
    fn from(metric: Metric) -> Record {
        match metric {
            Metric::Counter { name, val, .. } | Metric::Gauge { name, val, .. } => {
                let mut record = Record::new_empty();
                record.insert_explicit(name.into(), val.to_string().into());
                record
            }
            _ => Record::from(format!("{:?}", metric)),
        }
    }
}

#[cfg(test)]
mod test {
    use super::StatsdConfig;
    use crate::{
        sinks::prometheus::{Counter, Gauge, PrometheusSinkConfig},
        test_util::{block_on, next_addr, shutdown_on_idle},
        topology::{self, config},
    };
    use futures::Stream;
    use std::{thread, time::Duration};

    #[test]
    fn test_statsd() {
        let in_addr = next_addr();
        let out_addr = next_addr();

        let mut config = config::Config::empty();
        config.add_source("in", StatsdConfig { address: in_addr });
        config.add_sink(
            "out",
            &["in"],
            PrometheusSinkConfig {
                address: out_addr,
                counters: vec![Counter {
                    key: "foo".into(),
                    label: "foo".into(),
                    doc: "foo".into(),
                    parse_value: true,
                }],
                gauges: vec![Gauge {
                    key: "bar".into(),
                    label: "bar".into(),
                    doc: "bar".into(),
                }],
            },
        );

        let mut rt = tokio::runtime::Runtime::new().unwrap();

        let (topology, _crash) = topology::start(Ok(config), &mut rt, false).unwrap();

        let bind_addr = next_addr();
        let socket = std::net::UdpSocket::bind(&bind_addr).unwrap();

        for _ in 0..100 {
            socket
                .send_to(b"foo:1|c\nbar:42|g\nfoo:1|c\n", &in_addr)
                .unwrap();
            // Space things out slightly to try to avoid dropped packets
            thread::sleep(Duration::from_millis(1));
        }

        // Give packets some time to flow through
        thread::sleep(Duration::from_millis(10));

        let client = hyper::Client::new();
        let response =
            block_on(client.get(format!("http://{}/metrics", out_addr).parse().unwrap())).unwrap();
        assert!(response.status().is_success());

        let body = block_on(response.into_body().concat2()).unwrap();
        let lines = std::str::from_utf8(&body)
            .unwrap()
            .lines()
            .collect::<Vec<_>>();

        let foo = lines
            .iter()
            .find(|s| s.starts_with("foo "))
            .map(|s| s.split_whitespace().nth(1).unwrap())
            .unwrap()
            .parse::<usize>()
            .unwrap();
        let bar = lines
            .iter()
            .find(|s| s.starts_with("bar "))
            .map(|s| s.split_whitespace().nth(1).unwrap())
            .unwrap()
            .parse::<usize>()
            .unwrap();

        assert_eq!(42, bar);
        // packets get lost :(
        assert!(foo % 2 == 0);
        assert!(foo > 180);

        // Shut down server
        block_on(topology.stop()).unwrap();
        shutdown_on_idle(rt);
    }
}
