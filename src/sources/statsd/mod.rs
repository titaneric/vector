use crate::{topology::config::GlobalOptions, Event};
use futures::{future, sync::mpsc, Future, Sink, Stream};
use parser::parse;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tokio::{
    self,
    codec::BytesCodec,
    net::{UdpFramed, UdpSocket},
};
use tracing::field;

pub mod parser;

#[derive(Deserialize, Serialize, Debug)]
struct StatsdConfig {
    address: SocketAddr,
}

#[typetag::serde(name = "statsd")]
impl crate::topology::config::SourceConfig for StatsdConfig {
    fn build(
        &self,
        _name: &str,
        _globals: &GlobalOptions,
        out: mpsc::Sender<Event>,
    ) -> crate::Result<super::Source> {
        Ok(statsd(self.address, out))
    }

    fn output_type(&self) -> crate::topology::config::DataType {
        crate::topology::config::DataType::Metric
    }

    fn source_type(&self) -> &'static str {
        "statsd"
    }
}

fn statsd(addr: SocketAddr, out: mpsc::Sender<Event>) -> super::Source {
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
                        .map(Event::Metric)
                        .collect::<Vec<_>>();
                    futures::stream::iter_ok::<_, std::io::Error>(metrics)
                })
                .flatten()
                .map_err(|e| error!("error reading datagram: {:?}", e));

            metrics_in.forward(out).map(|_| info!("finished sending"))
        }),
    )
}

#[cfg(test)]
mod test {
    use super::StatsdConfig;
    use crate::{
        sinks::prometheus::PrometheusSinkConfig,
        test_util::{block_on, next_addr, runtime, shutdown_on_idle},
        topology::{self, config},
    };
    use futures::Stream;
    use std::{thread, time::Duration};

    fn parse_count(lines: &Vec<&str>, prefix: &str) -> usize {
        lines
            .iter()
            .find(|s| s.starts_with(prefix))
            .map(|s| s.split_whitespace().nth(1).unwrap())
            .unwrap()
            .parse::<usize>()
            .unwrap()
    }

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
                namespace: "vector".into(),
                buckets: vec![1.0, 2.0, 4.0],
                flush_period_secs: 1,
            },
        );

        let mut rt = runtime();

        let (topology, _crash) = topology::start(config, &mut rt, false).unwrap();

        let bind_addr = next_addr();
        let socket = std::net::UdpSocket::bind(&bind_addr).unwrap();

        for _ in 0..100 {
            socket
                .send_to(
                    b"foo:1|c|#a,b:b\nbar:42|g\nfoo:1|c|#a,b:c\nglork:3|h|@0.1\nmilliglork:3000|ms|@0.1\nset:0|s\nset:1|s\n",
                    &in_addr,
                )
                .unwrap();
            // Space things out slightly to try to avoid dropped packets
            thread::sleep(Duration::from_millis(10));
        }

        // Give packets some time to flow through
        thread::sleep(Duration::from_millis(100));

        let client = hyper::Client::new();
        let response =
            block_on(client.get(format!("http://{}/metrics", out_addr).parse().unwrap())).unwrap();
        assert!(response.status().is_success());

        let body = block_on(response.into_body().concat2()).unwrap();
        let lines = std::str::from_utf8(&body)
            .unwrap()
            .lines()
            .collect::<Vec<_>>();

        // note that prometheus client reorders the labels
        let foo1 = parse_count(&lines, "vector_foo{a=\"true\",b=\"b\"");
        let foo2 = parse_count(&lines, "vector_foo{a=\"true\",b=\"c\"");
        // packets get lost :(
        assert!(foo1 > 90);
        assert!(foo2 > 90);

        let bar = parse_count(&lines, "vector_bar");
        assert_eq!(42, bar);

        assert_eq!(parse_count(&lines, "vector_glork_bucket{le=\"1\"}"), 0);
        assert_eq!(parse_count(&lines, "vector_glork_bucket{le=\"2\"}"), 0);
        assert!(parse_count(&lines, "vector_glork_bucket{le=\"4\"}") > 0);
        assert!(parse_count(&lines, "vector_glork_bucket{le=\"+Inf\"}") > 0);
        let glork_sum = parse_count(&lines, "vector_glork_sum");
        let glork_count = parse_count(&lines, "vector_glork_count");
        assert_eq!(glork_count * 3, glork_sum);

        assert_eq!(parse_count(&lines, "vector_milliglork_bucket{le=\"1\"}"), 0);
        assert_eq!(parse_count(&lines, "vector_milliglork_bucket{le=\"2\"}"), 0);
        assert!(parse_count(&lines, "vector_milliglork_bucket{le=\"4\"}") > 0);
        assert!(parse_count(&lines, "vector_milliglork_bucket{le=\"+Inf\"}") > 0);
        let milliglork_sum = parse_count(&lines, "vector_milliglork_sum");
        let milliglork_count = parse_count(&lines, "vector_milliglork_count");
        assert_eq!(milliglork_count * 3, milliglork_sum);

        // Set test
        // Flush could have occured
        assert!(parse_count(&lines, "vector_set") <= 2);

        // Flush test
        {
            // Wait for flush to happen
            thread::sleep(Duration::from_millis(2000));

            let response =
                block_on(client.get(format!("http://{}/metrics", out_addr).parse().unwrap()))
                    .unwrap();
            assert!(response.status().is_success());

            let body = block_on(response.into_body().concat2()).unwrap();
            let lines = std::str::from_utf8(&body)
                .unwrap()
                .lines()
                .collect::<Vec<_>>();

            // Check rested
            assert_eq!(parse_count(&lines, "vector_set"), 0);

            // Recheck that set is also reseted------------

            socket.send_to(b"set:0|s\nset:1|s\n", &in_addr).unwrap();
            // Space things out slightly to try to avoid dropped packets
            thread::sleep(Duration::from_millis(10));
            // Give packets some time to flow through
            thread::sleep(Duration::from_millis(100));

            let response =
                block_on(client.get(format!("http://{}/metrics", out_addr).parse().unwrap()))
                    .unwrap();
            assert!(response.status().is_success());

            let body = block_on(response.into_body().concat2()).unwrap();
            let lines = std::str::from_utf8(&body)
                .unwrap()
                .lines()
                .collect::<Vec<_>>();

            // Set test
            assert_eq!(parse_count(&lines, "vector_set"), 2);
        }

        // Shut down server
        block_on(topology.stop()).unwrap();
        shutdown_on_idle(rt);
    }
}
