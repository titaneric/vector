//! Prelude module for sinks which will re-export the symbols that most
//! stream based sinks are likely to use.

pub use crate::{
    codecs::{Encoder, EncodingConfig, Transformer},
    components::validation::{
        ExternalResource, HttpResourceConfig, ResourceDirection, ValidatableComponent,
        ValidationConfiguration,
    },
    config::{DataType, GenerateConfig, SinkConfig, SinkContext},
    event::{Event, LogEvent},
    internal_events::{SinkRequestBuildError, TemplateRenderingError},
    register_validatable_component,
    sinks::{
        util::{
            builder::SinkBuilderExt,
            encoding::{self, write_all},
            metadata::RequestMetadataBuilder,
            request_builder::{default_request_builder_concurrency_limit, EncodeResult},
            retries::{RetryAction, RetryLogic},
            service::{ServiceBuilderExt, Svc},
            BatchConfig, Compression, Concurrency, NoDefaultsBatchSettings, RequestBuilder,
            SinkBatchSettings, TowerRequestConfig,
        },
        Healthcheck, HealthcheckError,
    },
    template::{Template, TemplateParseError},
    tls::TlsConfig,
};
pub use async_trait::async_trait;
pub use futures::{future, future::BoxFuture, stream::BoxStream, FutureExt, StreamExt};
pub use tower::{Service, ServiceBuilder};
pub use vector_buffers::EventCount;
pub use vector_common::{
    finalization::{EventFinalizers, EventStatus, Finalizable},
    internal_event::{CountByteSize, TaggedEventsSent},
    json_size::JsonSize,
    request_metadata::{GetEventCountTags, GroupedCountByteSize, MetaDescriptive, RequestMetadata},
};
pub use vector_config::configurable_component;

pub use vector_core::{
    config::{telemetry, AcknowledgementsConfig, Input},
    event::Value,
    partition::Partitioner,
    schema::Requirement,
    sink::{StreamSink, VectorSink},
    stream::{BatcherSettings, DriverResponse},
    tls::TlsSettings,
    ByteSizeOf, EstimatedJsonEncodedSizeOf,
};
