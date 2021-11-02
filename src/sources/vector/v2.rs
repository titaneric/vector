use crate::{
    config::SourceContext,
    config::{DataType, GenerateConfig, Resource},
    internal_events::{EventsReceived, TcpBytesReceived},
    proto::vector as proto,
    shutdown::ShutdownSignalToken,
    sources::{util::AfterReadExt as _, Source},
    tls::{MaybeTlsIncomingStream, MaybeTlsSettings, TlsConfig},
    Pipeline,
};

use futures::{FutureExt, SinkExt, StreamExt, TryFutureExt};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpStream;
use tonic::{
    transport::{server::Connected, Certificate, Server},
    Request, Response, Status,
};
use vector_core::event::{BatchNotifier, BatchStatus, BatchStatusReceiver, Event};
use vector_core::ByteSizeOf;

#[derive(Debug, Clone)]
pub struct Service {
    pipeline: Pipeline,
    acknowledgements: bool,
}

#[tonic::async_trait]
impl proto::Service for Service {
    async fn push_events(
        &self,
        request: Request<proto::PushEventsRequest>,
    ) -> Result<Response<proto::PushEventsResponse>, Status> {
        let mut events: Vec<Event> = request
            .into_inner()
            .events
            .into_iter()
            .map(Event::from)
            .collect();

        emit!(&EventsReceived {
            count: events.len(),
            byte_size: events.size_of(),
        });

        let receiver = self.acknowledgements.then(|| {
            let (batch, receiver) = BatchNotifier::new_with_receiver();
            for event in &mut events {
                event.add_batch_notifier(Arc::clone(&batch));
            }

            receiver
        });

        self.pipeline
            .clone()
            .send_all(&mut futures::stream::iter(events).map(Ok))
            .map_err(|err| Status::unavailable(err.to_string()))
            .and_then(|_| handle_batch_status(receiver))
            .await?;

        Ok(Response::new(proto::PushEventsResponse {}))
    }

    // TODO: figure out a way to determine if the current Vector instance is "healthy".
    async fn health_check(
        &self,
        _: Request<proto::HealthCheckRequest>,
    ) -> Result<Response<proto::HealthCheckResponse>, Status> {
        let message = proto::HealthCheckResponse {
            status: proto::ServingStatus::Serving.into(),
        };

        Ok(Response::new(message))
    }
}

async fn handle_batch_status(receiver: Option<BatchStatusReceiver>) -> Result<(), Status> {
    let status = match receiver {
        Some(receiver) => receiver.await,
        None => BatchStatus::Delivered,
    };

    match status {
        BatchStatus::Errored => Err(Status::internal("Delivery error")),
        BatchStatus::Failed => Err(Status::data_loss("Delivery failed")),
        BatchStatus::Delivered => Ok(()),
    }
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct VectorConfig {
    pub address: SocketAddr,
    #[serde(default = "default_shutdown_timeout_secs")]
    pub shutdown_timeout_secs: u64,
    #[serde(default)]
    tls: Option<TlsConfig>,
}

const fn default_shutdown_timeout_secs() -> u64 {
    30
}

impl GenerateConfig for VectorConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            address: "0.0.0.0:6000".parse().unwrap(),
            shutdown_timeout_secs: default_shutdown_timeout_secs(),
            tls: None,
        })
        .unwrap()
    }
}

impl VectorConfig {
    pub(super) async fn build(&self, cx: SourceContext) -> crate::Result<Source> {
        let tls_settings = MaybeTlsSettings::from_config(&self.tls, true)?;

        let source = run(self.address, tls_settings, cx).map_err(|error| {
            error!(message = "Source future failed.", %error);
        });

        Ok(Box::pin(source))
    }

    pub(super) const fn output_type(&self) -> DataType {
        DataType::Any
    }

    pub(super) const fn source_type(&self) -> &'static str {
        "vector"
    }

    pub(super) fn resources(&self) -> Vec<Resource> {
        vec![Resource::tcp(self.address)]
    }
}

async fn run(
    address: SocketAddr,
    tls_settings: MaybeTlsSettings,
    cx: SourceContext,
) -> crate::Result<()> {
    let _span = crate::trace::current_span();

    let service = proto::Server::new(Service {
        pipeline: cx.out,
        acknowledgements: cx.acknowledgements,
    });
    let (tx, rx) = tokio::sync::oneshot::channel::<ShutdownSignalToken>();

    let listener = tls_settings.bind(&address).await?;
    let stream = listener.accept_stream().map(|result| {
        result.map(|socket| {
            let peer_addr = socket.connect_info().remote_addr.ip();
            socket.after_read(move |byte_size| {
                emit!(&TcpBytesReceived {
                    byte_size,
                    peer_addr
                })
            })
        })
    });

    Server::builder()
        .add_service(service)
        .serve_with_incoming_shutdown(stream, cx.shutdown.map(|token| tx.send(token).unwrap()))
        .await?;

    drop(rx.await);

    Ok(())
}

#[derive(Clone)]
pub struct MaybeTlsConnectInfo {
    pub remote_addr: SocketAddr,
    pub peer_certs: Option<Vec<Certificate>>,
}

impl Connected for MaybeTlsIncomingStream<TcpStream> {
    type ConnectInfo = MaybeTlsConnectInfo;

    fn connect_info(&self) -> Self::ConnectInfo {
        MaybeTlsConnectInfo {
            remote_addr: self.peer_addr(),
            peer_certs: self
                .ssl_stream()
                .and_then(|s| s.ssl().peer_cert_chain())
                .map(|s| {
                    s.into_iter()
                        .filter_map(|c| c.to_pem().ok())
                        .map(Certificate::from_pem)
                        .collect()
                }),
        }
    }
}

#[cfg(feature = "sinks-vector")]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SinkContext;
    use crate::sinks::vector::v2::VectorConfig as SinkConfig;
    use crate::test_util::{self, components};
    use crate::Pipeline;
    use shared::assert_event_data_eq;

    #[tokio::test]
    async fn receive_message() {
        let addr = test_util::next_addr();
        let config = format!(r#"address = "{}""#, addr);
        let source: VectorConfig = toml::from_str(&config).unwrap();

        components::init_test();
        let (tx, rx) = Pipeline::new_test();
        let server = source.build(SourceContext::new_test(tx)).await.unwrap();
        tokio::spawn(server);
        test_util::wait_for_tcp(addr).await;

        // Ideally, this would be a fully custom agent to send the data,
        // but the sink side already does such a test and this is good
        // to ensure interoperability.
        let config = format!(r#"address = "{}""#, addr);
        let sink: SinkConfig = toml::from_str(&config).unwrap();
        let cx = SinkContext::new_test();
        let (sink, _) = sink.build(cx).await.unwrap();

        let (events, stream) = test_util::random_events_with_stream(100, 100, None);
        sink.run(stream).await.unwrap();
        components::SOURCE_TESTS.assert(&components::TCP_SOURCE_TAGS);

        let output = test_util::collect_ready(rx).await;
        assert_event_data_eq!(events, output);
    }
}
