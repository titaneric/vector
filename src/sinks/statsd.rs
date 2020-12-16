#[cfg(unix)]
use crate::sinks::util::unix::UnixSinkConfig;
use crate::{
    config::{DataType, GenerateConfig, SinkConfig, SinkContext, SinkDescription},
    event::metric::{Metric, MetricKind, MetricValue, StatisticKind},
    internal_events::StatsdInvalidMetricReceived,
    sinks::util::{
        encode_namespace,
        tcp::TcpSinkConfig,
        udp::{UdpService, UdpSinkConfig},
        BatchConfig, BatchSettings, BatchSink, Buffer, Compression,
    },
    Event,
};
use futures::{future, stream, FutureExt, SinkExt, StreamExt, TryFutureExt};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fmt::Display,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    task::{Context, Poll},
};
use tower::{Service, ServiceBuilder};

pub struct StatsdSvc {
    inner: UdpService,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
// TODO: add back when serde-rs/serde#1358 is addressed
// #[serde(deny_unknown_fields)]
pub struct StatsdSinkConfig {
    #[serde(alias = "namespace")]
    pub default_namespace: Option<String>,
    #[serde(flatten)]
    pub mode: Mode,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum Mode {
    Tcp(TcpSinkConfig),
    Udp(StatsdUdpConfig),
    #[cfg(unix)]
    Unix(UnixSinkConfig),
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct StatsdUdpConfig {
    #[serde(flatten)]
    pub udp: UdpSinkConfig,

    #[serde(default)]
    pub batch: BatchConfig,
}

inventory::submit! {
    SinkDescription::new::<StatsdSinkConfig>("statsd")
}

fn default_address() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8125)
}

impl GenerateConfig for StatsdSinkConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(&Self {
            default_namespace: None,
            mode: Mode::Udp(StatsdUdpConfig {
                batch: Default::default(),
                udp: UdpSinkConfig {
                    address: default_address().to_string(),
                },
            }),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "statsd")]
impl SinkConfig for StatsdSinkConfig {
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        let default_namespace = self.default_namespace.clone();
        match &self.mode {
            Mode::Tcp(config) => {
                let encode_event =
                    move |event| encode_event(event, default_namespace.as_deref()).map(Into::into);
                config.build(cx, encode_event)
            }
            Mode::Udp(config) => {
                // 1432 bytes is a recommended packet size to fit into MTU
                // https://github.com/statsd/statsd/blob/master/docs/metric_types.md#multi-metric-packets
                // However we need to leave some space for +1 extra trailing event in the buffer.
                // Also one might keep an eye on server side limitations, like
                // mentioned here https://github.com/DataDog/dd-agent/issues/2638
                let batch = BatchSettings::default()
                    .bytes(1300)
                    .events(1000)
                    .timeout(1)
                    .parse_config(config.batch)?;
                let (service, healthcheck) = config.udp.build_service(cx.clone())?;
                let service = StatsdSvc { inner: service };
                let sink = BatchSink::new(
                    ServiceBuilder::new().service(service),
                    Buffer::new(batch.size, Compression::None),
                    batch.timeout,
                    cx.acker(),
                )
                .sink_map_err(|error| error!(message = "Fatal statsd sink error.", %error))
                .with_flat_map(move |event| {
                    stream::iter(encode_event(event, default_namespace.as_deref())).map(Ok)
                });

                Ok((super::VectorSink::Sink(Box::new(sink)), healthcheck))
            }
            #[cfg(unix)]
            Mode::Unix(config) => {
                let encode_event =
                    move |event| encode_event(event, default_namespace.as_deref()).map(Into::into);
                config.build(cx, encode_event)
            }
        }
    }

    fn input_type(&self) -> DataType {
        DataType::Metric
    }

    fn sink_type(&self) -> &'static str {
        "statsd"
    }
}

fn encode_tags(tags: &BTreeMap<String, String>) -> String {
    let parts: Vec<_> = tags
        .iter()
        .map(|(name, value)| {
            if value == "true" {
                name.to_string()
            } else {
                format!("{}:{}", name, value)
            }
        })
        .collect();
    // `parts` is already sorted by key because of BTreeMap
    parts.join(",")
}

fn push_event<V: Display>(
    buf: &mut Vec<String>,
    metric: &Metric,
    val: V,
    metric_type: &str,
    sample_rate: Option<u32>,
) {
    buf.push(format!("{}:{}|{}", metric.name, val, metric_type));

    if let Some(sample_rate) = sample_rate {
        if sample_rate != 1 {
            buf.push(format!("@{}", 1.0 / f64::from(sample_rate)))
        }
    };

    if let Some(t) = &metric.tags {
        buf.push(format!("#{}", encode_tags(t)));
    };
}

fn encode_event(event: Event, default_namespace: Option<&str>) -> Option<Vec<u8>> {
    let mut buf = Vec::new();

    let metric = event.as_metric();
    match &metric.value {
        MetricValue::Counter { value } => {
            push_event(&mut buf, &metric, value, "c", None);
        }
        MetricValue::Gauge { value } => {
            match metric.kind {
                MetricKind::Incremental => {
                    push_event(&mut buf, &metric, format!("{:+}", value), "g", None)
                }
                MetricKind::Absolute => push_event(&mut buf, &metric, value, "g", None),
            };
        }
        MetricValue::Distribution {
            values,
            sample_rates,
            statistic,
        } => {
            let metric_type = match statistic {
                StatisticKind::Histogram => "h",
                StatisticKind::Summary => "d",
            };
            for (val, sample_rate) in values.iter().zip(sample_rates.iter()) {
                push_event(&mut buf, &metric, val, metric_type, Some(*sample_rate));
            }
        }
        MetricValue::Set { values } => {
            for val in values {
                push_event(&mut buf, &metric, val, "s", None);
            }
        }
        _ => {
            emit!(StatsdInvalidMetricReceived {
                value: &metric.value,
                kind: &metric.kind,
            });

            return None;
        }
    };

    let message = encode_namespace(
        metric.namespace.as_deref().or(default_namespace),
        '.',
        buf.join("|"),
    );

    let mut body: Vec<u8> = message.into_bytes();
    body.push(b'\n');

    Some(body)
}

impl Service<Vec<u8>> for StatsdSvc {
    type Response = ();
    type Error = crate::Error;
    type Future = future::BoxFuture<'static, Result<(), Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx).map_err(Into::into)
    }

    fn call(&mut self, frame: Vec<u8>) -> Self::Future {
        self.inner.call(frame.into()).err_into().boxed()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{event::Metric, test_util::*};
    use bytes::Bytes;
    use futures::TryStreamExt;
    use tokio::net::UdpSocket;
    use tokio::sync::mpsc;
    use tokio_util::{codec::BytesCodec, udp::UdpFramed};

    #[cfg(feature = "sources-statsd")]
    use {crate::sources::statsd::parser::parse, std::str::from_utf8};

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<StatsdSinkConfig>();
    }

    fn tags() -> BTreeMap<String, String> {
        vec![
            ("normal_tag".to_owned(), "value".to_owned()),
            ("true_tag".to_owned(), "true".to_owned()),
            ("empty_tag".to_owned(), "".to_owned()),
        ]
        .into_iter()
        .collect()
    }

    #[test]
    fn test_encode_tags() {
        assert_eq!(
            &encode_tags(&tags()),
            "empty_tag:,normal_tag:value,true_tag"
        );
    }

    #[test]
    fn tags_order() {
        assert_eq!(
            &encode_tags(
                &vec![
                    ("a", "value"),
                    ("b", "value"),
                    ("c", "value"),
                    ("d", "value"),
                    ("e", "value"),
                ]
                .into_iter()
                .map(|(k, v)| (k.to_owned(), v.to_owned()))
                .collect()
            ),
            "a:value,b:value,c:value,d:value,e:value"
        );
    }

    #[cfg(feature = "sources-statsd")]
    #[test]
    fn test_encode_counter() {
        let metric1 = Metric {
            name: "counter".to_owned(),
            namespace: None,
            timestamp: None,
            tags: Some(tags()),
            kind: MetricKind::Incremental,
            value: MetricValue::Counter { value: 1.5 },
        };
        let event = Event::Metric(metric1.clone());
        let frame = &encode_event(event, None).unwrap();
        let metric2 = parse(from_utf8(&frame).unwrap().trim()).unwrap();
        assert_eq!(metric1, metric2);
    }

    #[cfg(feature = "sources-statsd")]
    #[test]
    fn test_encode_absolute_counter() {
        let metric1 = Metric {
            name: "counter".to_owned(),
            namespace: None,
            timestamp: None,
            tags: None,
            kind: MetricKind::Absolute,
            value: MetricValue::Counter { value: 1.5 },
        };
        let event = Event::Metric(metric1);
        let frame = &encode_event(event, None).unwrap();
        // The statsd parser will parse the counter as Incremental,
        // so we can't compare it with the parsed value.
        assert_eq!("counter:1.5|c\n", from_utf8(&frame).unwrap());
    }

    #[cfg(feature = "sources-statsd")]
    #[test]
    fn test_encode_gauge() {
        let metric1 = Metric {
            name: "gauge".to_owned(),
            namespace: None,
            timestamp: None,
            tags: Some(tags()),
            kind: MetricKind::Incremental,
            value: MetricValue::Gauge { value: -1.5 },
        };
        let event = Event::Metric(metric1.clone());
        let frame = &encode_event(event, None).unwrap();
        let metric2 = parse(from_utf8(&frame).unwrap().trim()).unwrap();
        assert_eq!(metric1, metric2);
    }

    #[cfg(feature = "sources-statsd")]
    #[test]
    fn test_encode_absolute_gauge() {
        let metric1 = Metric {
            name: "gauge".to_owned(),
            namespace: None,
            timestamp: None,
            tags: Some(tags()),
            kind: MetricKind::Absolute,
            value: MetricValue::Gauge { value: 1.5 },
        };
        let event = Event::Metric(metric1.clone());
        let frame = &encode_event(event, None).unwrap();
        let metric2 = parse(from_utf8(&frame).unwrap().trim()).unwrap();
        assert_eq!(metric1, metric2);
    }

    #[cfg(feature = "sources-statsd")]
    #[test]
    fn test_encode_distribution() {
        let metric1 = Metric {
            name: "distribution".to_owned(),
            namespace: None,
            timestamp: None,
            tags: Some(tags()),
            kind: MetricKind::Incremental,
            value: MetricValue::Distribution {
                values: vec![1.5],
                sample_rates: vec![1],
                statistic: StatisticKind::Histogram,
            },
        };
        let event = Event::Metric(metric1.clone());
        let frame = &encode_event(event, None).unwrap();
        let metric2 = parse(from_utf8(&frame).unwrap().trim()).unwrap();
        assert_eq!(metric1, metric2);
    }

    #[cfg(feature = "sources-statsd")]
    #[test]
    fn test_encode_set() {
        let metric1 = Metric {
            name: "set".to_owned(),
            namespace: None,
            timestamp: None,
            tags: Some(tags()),
            kind: MetricKind::Incremental,
            value: MetricValue::Set {
                values: vec!["abc".to_owned()].into_iter().collect(),
            },
        };
        let event = Event::Metric(metric1.clone());
        let frame = &encode_event(event, None).unwrap();
        let metric2 = parse(from_utf8(&frame).unwrap().trim()).unwrap();
        assert_eq!(metric1, metric2);
    }

    #[tokio::test]
    async fn test_send_to_statsd() {
        trace_init();

        let addr = next_addr();

        let config = StatsdSinkConfig {
            default_namespace: Some("ns".into()),
            mode: Mode::Udp(StatsdUdpConfig {
                batch: BatchConfig {
                    max_bytes: Some(512),
                    timeout_secs: Some(1),
                    ..Default::default()
                },
                udp: UdpSinkConfig {
                    address: addr.to_string(),
                },
            }),
        };

        let context = SinkContext::new_test();
        let (sink, _healthcheck) = config.build(context).await.unwrap();

        let events = vec![
            Event::Metric(Metric {
                name: "counter".to_owned(),
                namespace: Some("vector".into()),
                timestamp: None,
                tags: Some(tags()),
                kind: MetricKind::Incremental,
                value: MetricValue::Counter { value: 1.5 },
            }),
            Event::Metric(Metric {
                name: "histogram".to_owned(),
                namespace: Some("vector".into()),
                timestamp: None,
                tags: None,
                kind: MetricKind::Incremental,
                value: MetricValue::Distribution {
                    values: vec![2.0],
                    sample_rates: vec![100],
                    statistic: StatisticKind::Histogram,
                },
            }),
        ];
        let (mut tx, rx) = mpsc::channel(1);

        let socket = UdpSocket::bind(addr).await.unwrap();
        tokio::spawn(async move {
            let mut stream = UdpFramed::new(socket, BytesCodec::new())
                .map_err(|error| error!(message = "Error reading line.", %error))
                .map_ok(|(bytes, _addr)| bytes.freeze());

            while let Some(Ok(item)) = stream.next().await {
                tx.send(item).await.unwrap();
            }
        });

        sink.run(stream::iter(events)).await.unwrap();

        let messages = collect_n(rx, 1).await;
        assert_eq!(
            messages[0],
            Bytes::from("vector.counter:1.5|c|#empty_tag:,normal_tag:value,true_tag\nvector.histogram:2|h|@0.01\n"),
        );
    }
}
