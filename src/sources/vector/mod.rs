use std::net::SocketAddr;

use futures::TryFutureExt;
use tonic::{Request, Response, Status};
use vector_config::configurable_component;
use vector_core::{
    config::LogNamespace,
    event::{BatchNotifier, BatchStatus, BatchStatusReceiver, Event},
    ByteSizeOf,
};

use crate::{
    config::{
        AcknowledgementsConfig, DataType, GenerateConfig, Output, Resource, SourceConfig,
        SourceContext,
    },
    internal_events::{EventsReceived, StreamClosedError},
    proto::vector as proto,
    serde::bool_or_struct,
    sources::{util::grpc::run_grpc_server, Source},
    tls::{MaybeTlsSettings, TlsEnableableConfig},
    SourceSender,
};

/// Marker type for the version two of the configuration for the `vector` source.
#[configurable_component]
#[derive(Clone, Debug)]
enum VectorConfigVersion {
    /// Marker value for version two.
    #[serde(rename = "2")]
    V2,
}

#[derive(Debug, Clone)]
pub struct Service {
    pipeline: SourceSender,
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

        let count = events.len();
        let byte_size = events.size_of();

        emit!(EventsReceived { count, byte_size });

        let receiver = BatchNotifier::maybe_apply_to(self.acknowledgements, &mut events);

        self.pipeline
            .clone()
            .send_batch(events)
            .map_err(|error| {
                let message = error.to_string();
                emit!(StreamClosedError { error, count });
                Status::unavailable(message)
            })
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
        BatchStatus::Rejected => Err(Status::data_loss("Delivery failed")),
        BatchStatus::Delivered => Ok(()),
    }
}

/// Configuration for the `vector` source.
#[configurable_component(source("vector"))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct VectorConfig {
    /// Version of the configuration.
    version: Option<VectorConfigVersion>,

    /// The address to listen for connections on.
    ///
    /// It _must_ include a port.
    pub address: SocketAddr,

    #[configurable(derived)]
    #[serde(default)]
    tls: Option<TlsEnableableConfig>,

    #[configurable(derived)]
    #[serde(default, deserialize_with = "bool_or_struct")]
    acknowledgements: AcknowledgementsConfig,
}

impl GenerateConfig for VectorConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            version: None,
            address: "0.0.0.0:6000".parse().unwrap(),
            tls: None,
            acknowledgements: Default::default(),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
impl SourceConfig for VectorConfig {
    async fn build(&self, cx: SourceContext) -> crate::Result<Source> {
        let tls_settings = MaybeTlsSettings::from_config(&self.tls, true)?;
        let acknowledgements = cx.do_acknowledgements(&self.acknowledgements);
        let service = proto::Server::new(Service {
            pipeline: cx.out,
            acknowledgements,
        })
        .accept_compressed(tonic::codec::CompressionEncoding::Gzip);

        let source =
            run_grpc_server(self.address, tls_settings, service, cx.shutdown).map_err(|error| {
                error!(message = "Source future failed.", %error);
            });

        Ok(Box::pin(source))
    }

    fn outputs(&self, _: LogNamespace) -> Vec<Output> {
        vec![Output::default(DataType::all())]
    }

    fn resources(&self) -> Vec<Resource> {
        vec![Resource::tcp(self.address)]
    }

    fn can_acknowledge(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<super::VectorConfig>();
    }
}

#[cfg(feature = "sinks-vector")]
#[cfg(test)]
mod tests {
    use vector_common::assert_event_data_eq;

    use super::*;
    use crate::{
        config::{SinkConfig as _, SinkContext},
        sinks::vector::VectorConfig as SinkConfig,
        test_util::{
            self,
            components::{assert_source_compliance, SOURCE_TAGS},
        },
        SourceSender,
    };

    #[tokio::test]
    async fn receive_message() {
        assert_source_compliance(&SOURCE_TAGS, async {
            let addr = test_util::next_addr();
            let config = format!(r#"address = "{}""#, addr);
            let source: VectorConfig = toml::from_str(&config).unwrap();

            let (tx, rx) = SourceSender::new_test();
            let server = source
                .build(SourceContext::new_test(tx, None))
                .await
                .unwrap();
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

            let output = test_util::collect_ready(rx).await;
            assert_event_data_eq!(events, output);
        })
        .await;
    }

    #[tokio::test]
    async fn receive_compressed_message() {
        assert_source_compliance(&SOURCE_TAGS, async {
            let addr = test_util::next_addr();
            let config = format!(r#"address = "{}""#, addr);
            let source: VectorConfig = toml::from_str(&config).unwrap();

            let (tx, rx) = SourceSender::new_test();
            let server = source
                .build(SourceContext::new_test(tx, None))
                .await
                .unwrap();
            tokio::spawn(server);
            test_util::wait_for_tcp(addr).await;

            // Ideally, this would be a fully custom agent to send the data,
            // but the sink side already does such a test and this is good
            // to ensure interoperability.
            let config = format!(
                r#"address = "{}"
            compression=true"#,
                addr
            );
            let sink: SinkConfig = toml::from_str(&config).unwrap();
            let cx = SinkContext::new_test();
            let (sink, _) = sink.build(cx).await.unwrap();

            let (events, stream) = test_util::random_events_with_stream(100, 100, None);
            sink.run(stream).await.unwrap();

            let output = test_util::collect_ready(rx).await;
            assert_event_data_eq!(events, output);
        })
        .await;
    }
}
