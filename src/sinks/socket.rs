#[cfg(unix)]
use crate::sinks::util::unix::UnixSinkConfig;
use crate::{
    config::{DataType, GenerateConfig, SinkConfig, SinkContext, SinkDescription},
    sinks::util::{
        encode_event, encoding::EncodingConfig, tcp::TcpSinkConfig, udp::UdpSinkConfig, Encoding,
    },
    tls::TlsConfig,
};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug)]
// TODO: add back when serde-rs/serde#1358 is addressed
// #[serde(deny_unknown_fields)]
pub struct SocketSinkConfig {
    #[serde(flatten)]
    pub mode: Mode,
    pub encoding: EncodingConfig<Encoding>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum Mode {
    Tcp(TcpSinkConfig),
    Udp(UdpSinkConfig),
    #[cfg(unix)]
    Unix(UnixSinkConfig),
}

inventory::submit! {
    SinkDescription::new::<SocketSinkConfig>("socket")
}

impl GenerateConfig for SocketSinkConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"address = "92.12.333.224:5000"
            mode = "tcp"
            encoding.codec = "json""#,
        )
        .unwrap()
    }
}

impl SocketSinkConfig {
    pub fn make_tcp_config(
        address: String,
        encoding: EncodingConfig<Encoding>,
        tls: Option<TlsConfig>,
    ) -> Self {
        SocketSinkConfig {
            mode: Mode::Tcp(TcpSinkConfig { address, tls }),
            encoding,
        }
    }

    pub fn make_basic_tcp_config(address: String) -> Self {
        Self::make_tcp_config(address, EncodingConfig::from(Encoding::Text), None)
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "socket")]
impl SinkConfig for SocketSinkConfig {
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        let encoding = self.encoding.clone();
        let encode_event = move |event| encode_event(event, &encoding);
        match &self.mode {
            Mode::Tcp(config) => config.build(cx, encode_event),
            Mode::Udp(config) => config.build(cx, encode_event),
            #[cfg(unix)]
            Mode::Unix(config) => config.build(cx, encode_event),
        }
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "socket"
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        config::SinkContext,
        event::Event,
        test_util::{next_addr, next_addr_v6, random_lines_with_stream, trace_init, CountReceiver},
    };
    use futures::stream::{self, StreamExt};
    use serde_json::Value;
    use std::{
        future::ready,
        net::{SocketAddr, UdpSocket},
    };
    use tokio::{
        net::TcpListener,
        time::{delay_for, timeout, Duration},
    };
    use tokio_util::codec::{FramedRead, LinesCodec};

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<SocketSinkConfig>();
    }

    async fn test_udp(addr: SocketAddr) {
        let receiver = UdpSocket::bind(addr).unwrap();

        let config = SocketSinkConfig {
            mode: Mode::Udp(UdpSinkConfig {
                address: addr.to_string(),
            }),
            encoding: Encoding::Json.into(),
        };
        let context = SinkContext::new_test();
        let (sink, _healthcheck) = config.build(context).await.unwrap();

        let event = Event::from("raw log line");
        sink.run(stream::once(ready(event))).await.unwrap();

        let mut buf = [0; 256];
        let (size, _src_addr) = receiver
            .recv_from(&mut buf)
            .expect("Did not receive message");

        let packet = String::from_utf8(buf[..size].to_vec()).expect("Invalid data received");
        let data = serde_json::from_str::<Value>(&packet).expect("Invalid JSON received");
        let data = data.as_object().expect("Not a JSON object");
        assert!(data.get("timestamp").is_some());
        let message = data.get("message").expect("No message in JSON");
        assert_eq!(message, &Value::String("raw log line".into()));
    }

    #[tokio::test]
    async fn udp_ipv4() {
        trace_init();

        test_udp(next_addr()).await;
    }

    #[tokio::test]
    async fn udp_ipv6() {
        trace_init();

        test_udp(next_addr_v6()).await;
    }

    #[tokio::test]
    async fn tcp_stream() {
        trace_init();

        let addr = next_addr();
        let config = SocketSinkConfig {
            mode: Mode::Tcp(TcpSinkConfig {
                address: addr.to_string(),
                tls: None,
            }),
            encoding: Encoding::Json.into(),
        };

        let context = SinkContext::new_test();
        let (sink, _healthcheck) = config.build(context).await.unwrap();

        let mut receiver = CountReceiver::receive_lines(addr);

        let (lines, events) = random_lines_with_stream(10, 100);
        sink.run(events).await.unwrap();

        // Wait for output to connect
        receiver.connected().await;

        let output = receiver.await;
        assert_eq!(lines.len(), output.len());
        for (source, received) in lines.iter().zip(output) {
            let json = serde_json::from_str::<Value>(&received).expect("Invalid JSON");
            let received = json.get("message").unwrap().as_str().unwrap();
            assert_eq!(source, received);
        }
    }

    // This is a test that checks that we properly receive all events in the
    // case of a proper server side write side shutdown.
    //
    // This test basically sends 10 events, shuts down the server and forces a
    // reconnect. It then forces another 10 events through and we should get a
    // total of 20 events.
    //
    // If this test hangs that means somewhere we are not collecting the correct
    // events.
    #[cfg(all(feature = "sources-utils-tls", feature = "listenfd"))]
    #[tokio::test]
    async fn tcp_stream_detects_disconnect() {
        use crate::tls::{self, MaybeTlsIncomingStream, MaybeTlsSettings, TlsConfig, TlsOptions};
        use futures::{future, FutureExt, StreamExt};
        use std::{
            net::Shutdown,
            pin::Pin,
            sync::{
                atomic::{AtomicUsize, Ordering},
                Arc,
            },
            task::Poll,
        };
        use tokio::{
            io::AsyncRead,
            net::TcpStream,
            sync::mpsc,
            task::yield_now,
            time::{interval, Duration},
        };

        trace_init();

        let addr = next_addr();
        let config = SocketSinkConfig {
            mode: Mode::Tcp(TcpSinkConfig {
                address: addr.to_string(),
                tls: Some(TlsConfig {
                    enabled: Some(true),
                    options: TlsOptions {
                        verify_certificate: Some(false),
                        verify_hostname: Some(false),
                        ca_file: Some(tls::TEST_PEM_CRT_PATH.into()),
                        ..Default::default()
                    },
                }),
            }),
            encoding: Encoding::Text.into(),
        };
        let context = SinkContext::new_test();
        let (sink, _healthcheck) = config.build(context).await.unwrap();
        let (mut sender, receiver) = mpsc::channel::<Option<Event>>(1);
        let jh1 = tokio::spawn(async move {
            let stream = receiver
                .take_while(|event| ready(event.is_some()))
                .map(|event| event.unwrap())
                .boxed();
            let _ = sink.run(stream).await.unwrap();
        });

        let msg_counter = Arc::new(AtomicUsize::new(0));
        let msg_counter1 = Arc::clone(&msg_counter);
        let conn_counter = Arc::new(AtomicUsize::new(0));
        let conn_counter1 = Arc::clone(&conn_counter);

        let (close_tx, close_rx) = tokio::sync::oneshot::channel::<()>();
        let mut close_rx = Some(close_rx.map(|x| x.unwrap()));

        let config = Some(TlsConfig::test_config());

        // Only accept two connections.
        let jh2 = tokio::spawn(async move {
            let tls = MaybeTlsSettings::from_config(&config, true).unwrap();
            let listener = tls.bind(&addr).await.unwrap();
            listener
                .accept_stream()
                .take(2)
                .for_each(|connection| {
                    let mut close_rx = close_rx.take();

                    conn_counter1.fetch_add(1, Ordering::SeqCst);
                    let msg_counter1 = Arc::clone(&msg_counter1);

                    let mut stream: MaybeTlsIncomingStream<TcpStream> = connection.unwrap();
                    future::poll_fn(move |cx| loop {
                        if let Some(fut) = close_rx.as_mut() {
                            if let Poll::Ready(()) = fut.poll_unpin(cx) {
                                stream.get_ref().unwrap().shutdown(Shutdown::Write).unwrap();
                                close_rx = None;
                            }
                        }

                        return match Pin::new(&mut stream).poll_read(cx, &mut [0u8; 11]) {
                            Poll::Ready(Ok(n)) => {
                                if n == 0 {
                                    Poll::Ready(())
                                } else {
                                    msg_counter1.fetch_add(1, Ordering::SeqCst);
                                    continue;
                                }
                            }
                            Poll::Ready(Err(error)) => panic!("{}", error),
                            Poll::Pending => Poll::Pending,
                        };
                    })
                })
                .await;
        });

        let (_, mut events) = random_lines_with_stream(10, 10);
        while let Some(event) = events.next().await {
            let _ = sender.send(Some(event)).await.unwrap();
        }

        // Loop and check for 10 events, we should always get 10 events. Once,
        // we have 10 events we can tell the server to shutdown to simulate the
        // remote shutting down on an idle connection.
        interval(Duration::from_millis(100))
            .take(500)
            .take_while(|_| ready(msg_counter.load(Ordering::SeqCst) != 10))
            .for_each(|_| ready(()))
            .await;
        close_tx.send(()).unwrap();

        // Close connection in spawned future
        yield_now().await;

        assert_eq!(msg_counter.load(Ordering::SeqCst), 10);
        assert_eq!(conn_counter.load(Ordering::SeqCst), 1);

        // Send another 10 events
        let (_, mut events) = random_lines_with_stream(10, 10);
        while let Some(event) = events.next().await {
            let _ = sender.send(Some(event)).await.unwrap();
        }

        // Wait for server task to be complete.
        let _ = sender.send(None).await.unwrap();
        let _ = jh1.await.unwrap();
        let _ = jh2.await.unwrap();

        // Check that there are exactly 20 events.
        assert_eq!(msg_counter.load(Ordering::SeqCst), 20);
        assert_eq!(conn_counter.load(Ordering::SeqCst), 2);
    }

    /// Tests whether socket recovers from a hard disconnect.
    #[tokio::test]
    async fn reconnect() {
        trace_init();

        let addr = next_addr();
        let config = SocketSinkConfig {
            mode: Mode::Tcp(TcpSinkConfig {
                address: addr.to_string(),
                tls: None,
            }),
            encoding: Encoding::Text.into(),
        };

        let context = SinkContext::new_test();
        let (sink, _healthcheck) = config.build(context).await.unwrap();

        let (_, events) = random_lines_with_stream(1000, 10000);
        let _ = tokio::spawn(sink.run(events));

        // First listener
        let mut count = 20usize;
        TcpListener::bind(addr)
            .await
            .unwrap()
            .next()
            .await
            .unwrap()
            .map(|socket| FramedRead::new(socket, LinesCodec::new()))
            .unwrap()
            .map(|x| x.unwrap())
            .take_while(|_| {
                ready(if count > 0 {
                    count -= 1;
                    true
                } else {
                    false
                })
            })
            .collect::<Vec<_>>()
            .await;

        // Disconnect
        if cfg!(windows) {
            // Gives Windows time to release the addr port.
            delay_for(Duration::from_secs(1)).await;
        }

        // Second listener
        // If this doesn't succeed then the sink hanged.
        assert!(timeout(
            Duration::from_secs(5),
            CountReceiver::receive_lines(addr).connected()
        )
        .await
        .is_ok());
    }
}
