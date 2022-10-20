use std::{collections::HashMap, convert::TryFrom, io};

use bytes::Bytes;
use chrono::Utc;
use codecs::encoding::Framer;
use http::header::{HeaderName, HeaderValue};
use indoc::indoc;
use snafu::ResultExt;
use snafu::Snafu;
use tower::ServiceBuilder;
use uuid::Uuid;
use vector_config::configurable_component;
use vector_core::event::{EventFinalizers, Finalizable};

use crate::{
    codecs::{Encoder, EncodingConfigWithFraming, SinkType, Transformer},
    config::{AcknowledgementsConfig, DataType, GenerateConfig, Input, SinkConfig, SinkContext},
    event::Event,
    gcp::{GcpAuthConfig, GcpAuthenticator, Scope},
    http::HttpClient,
    serde::json::to_string,
    sinks::{
        gcs_common::{
            config::{
                build_healthcheck, GcsPredefinedAcl, GcsRetryLogic, GcsStorageClass, BASE_URL,
            },
            service::{GcsRequest, GcsRequestSettings, GcsService},
            sink::GcsSink,
        },
        util::{
            batch::BatchConfig,
            metadata::{RequestMetadata, RequestMetadataBuilder},
            partitioner::KeyPartitioner,
            request_builder::EncodeResult,
            BulkSizeBasedDefaultBatchSettings, Compression, RequestBuilder, ServiceBuilderExt,
            TowerRequestConfig,
        },
        Healthcheck, VectorSink,
    },
    template::{Template, TemplateParseError},
    tls::{TlsConfig, TlsSettings},
};

#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum GcsHealthcheckError {
    #[snafu(display("key_prefix template parse error: {}", source))]
    KeyPrefixTemplate { source: TemplateParseError },
}

/// Configuration for the `gcp_cloud_storage` sink.
#[configurable_component(sink("gcp_cloud_storage"))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct GcsSinkConfig {
    /// The GCS bucket name.
    bucket: String,

    /// The Predefined ACL to apply to created objects.
    ///
    /// For more information, see [Predefined ACLs][predefined_acls].
    ///
    /// [predefined_acls]: https://cloud.google.com/storage/docs/access-control/lists#predefined-acl
    acl: Option<GcsPredefinedAcl>,

    /// The storage class for created objects.
    ///
    /// For more information, see [Storage classes][storage_classes].
    ///
    /// [storage_classes]: https://cloud.google.com/storage/docs/storage-classes
    storage_class: Option<GcsStorageClass>,

    /// The set of metadata `key:value` pairs for the created objects.
    ///
    /// For more information, see [Custom metadata][custom_metadata].
    ///
    /// [custom_metadata]: https://cloud.google.com/storage/docs/metadata#custom-metadata
    metadata: Option<HashMap<String, String>>,

    /// A prefix to apply to all object keys.
    ///
    /// Prefixes are useful for partitioning objects, such as by creating an object key that
    /// stores objects under a particular "directory". If using a prefix for this purpose, it must end
    /// in `/` in order to act as a directory path: Vector will **not** add a trailing `/` automatically.
    #[configurable(metadata(docs::templateable))]
    key_prefix: Option<String>,

    /// The timestamp format for the time component of the object key.
    ///
    /// By default, object keys are appended with a timestamp that reflects when the objects are
    /// sent to S3, such that the resulting object key is functionally equivalent to joining the key
    /// prefix with the formatted timestamp, such as `date=2022-07-18/1658176486`.
    ///
    /// This would represent a `key_prefix` set to `date=%F/` and the timestamp of Mon Jul 18 2022
    /// 20:34:44 GMT+0000, with the `filename_time_format` being set to `%s`, which renders
    /// timestamps in seconds since the Unix epoch.
    ///
    /// Supports the common [`strftime`][chrono_strftime_specifiers] specifiers found in most
    /// languages.
    ///
    /// When set to an empty string, no timestamp will be appended to the key prefix.
    ///
    /// [chrono_strftime_specifiers]: https://docs.rs/chrono/latest/chrono/format/strftime/index.html#specifiers
    filename_time_format: Option<String>,

    /// Whether or not to append a UUID v4 token to the end of the object key.
    ///
    /// The UUID is appended to the timestamp portion of the object key, such that if the object key
    /// being generated was `date=2022-07-18/1658176486`, setting this field to `true` would result
    /// in an object key that looked like `date=2022-07-18/1658176486-30f6652c-71da-4f9f-800d-a1189c47c547`.
    ///
    /// This ensures there are no name collisions, and can be useful in high-volume workloads where
    /// object keys must be unique.
    filename_append_uuid: Option<bool>,

    /// The filename extension to use in the object key.
    filename_extension: Option<String>,

    #[serde(flatten)]
    encoding: EncodingConfigWithFraming,

    #[configurable(derived)]
    #[serde(default)]
    compression: Compression,

    #[configurable(derived)]
    #[serde(default)]
    batch: BatchConfig<BulkSizeBasedDefaultBatchSettings>,

    #[configurable(derived)]
    #[serde(default)]
    request: TowerRequestConfig,

    #[serde(flatten)]
    auth: GcpAuthConfig,

    #[configurable(derived)]
    tls: Option<TlsConfig>,

    #[configurable(derived)]
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    acknowledgements: AcknowledgementsConfig,
}

#[cfg(test)]
fn default_config(encoding: EncodingConfigWithFraming) -> GcsSinkConfig {
    GcsSinkConfig {
        bucket: Default::default(),
        acl: Default::default(),
        storage_class: Default::default(),
        metadata: Default::default(),
        key_prefix: Default::default(),
        filename_time_format: Default::default(),
        filename_append_uuid: Default::default(),
        filename_extension: Default::default(),
        encoding,
        compression: Compression::gzip_default(),
        batch: Default::default(),
        request: Default::default(),
        auth: Default::default(),
        tls: Default::default(),
        acknowledgements: Default::default(),
    }
}

impl GenerateConfig for GcsSinkConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(indoc! {r#"
            bucket = "my-bucket"
            credentials_path = "/path/to/credentials.json"
            framing.method = "newline_delimited"
            encoding.codec = "json"
        "#})
        .unwrap()
    }
}

#[async_trait::async_trait]
impl SinkConfig for GcsSinkConfig {
    async fn build(&self, cx: SinkContext) -> crate::Result<(VectorSink, Healthcheck)> {
        let auth = self.auth.build(Scope::DevStorageReadWrite).await?;
        let base_url = format!("{}{}/", BASE_URL, self.bucket);
        let tls = TlsSettings::from_options(&self.tls)?;
        let client = HttpClient::new(tls, cx.proxy())?;
        let healthcheck = build_healthcheck(
            self.bucket.clone(),
            client.clone(),
            base_url.clone(),
            auth.clone(),
        )?;
        let sink = self.build_sink(client, base_url, auth)?;

        Ok((sink, healthcheck))
    }

    fn input(&self) -> Input {
        Input::new(self.encoding.config().1.input_type() & DataType::Log)
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        &self.acknowledgements
    }
}

impl GcsSinkConfig {
    fn build_sink(
        &self,
        client: HttpClient,
        base_url: String,
        auth: GcpAuthenticator,
    ) -> crate::Result<VectorSink> {
        let request = self.request.unwrap_with(&TowerRequestConfig {
            rate_limit_num: Some(1000),
            ..Default::default()
        });

        let batch_settings = self.batch.into_batcher_settings()?;

        let partitioner = self.key_partitioner()?;

        let svc = ServiceBuilder::new()
            .settings(request, GcsRetryLogic)
            .service(GcsService::new(client, base_url, auth));

        let request_settings = RequestSettings::new(self)?;

        let sink = GcsSink::new(svc, request_settings, partitioner, batch_settings);

        Ok(VectorSink::from_event_streamsink(sink))
    }

    fn key_partitioner(&self) -> crate::Result<KeyPartitioner> {
        Ok(KeyPartitioner::new(
            Template::try_from(self.key_prefix.as_deref().unwrap_or("date=%F/"))
                .context(KeyPrefixTemplateSnafu)?,
        ))
    }
}

// Settings required to produce a request that do not change per
// request. All possible values are pre-computed for direct use in
// producing a request.
#[derive(Clone, Debug)]
struct RequestSettings {
    acl: Option<HeaderValue>,
    content_type: HeaderValue,
    content_encoding: Option<HeaderValue>,
    storage_class: HeaderValue,
    headers: Vec<(HeaderName, HeaderValue)>,
    extension: String,
    time_format: String,
    append_uuid: bool,
    encoder: (Transformer, Encoder<Framer>),
    compression: Compression,
}

impl RequestBuilder<(String, Vec<Event>)> for RequestSettings {
    type Metadata = (String, EventFinalizers, RequestMetadataBuilder);
    type Events = Vec<Event>;
    type Encoder = (Transformer, Encoder<Framer>);
    type Payload = Bytes;
    type Request = GcsRequest;
    type Error = io::Error;

    fn compression(&self) -> Compression {
        self.compression
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoder
    }

    fn split_input(&self, input: (String, Vec<Event>)) -> (Self::Metadata, Self::Events) {
        let (partition_key, mut events) = input;
        let metadata_builder = RequestMetadata::builder(&events);
        let finalizers = events.take_finalizers();

        ((partition_key, finalizers, metadata_builder), events)
    }

    fn build_request(
        &self,
        metadata: Self::Metadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        let (key, finalizers, metadata_builder) = metadata;
        // TODO: pull the seconds from the last event
        let filename = {
            let seconds = Utc::now().format(&self.time_format);

            if self.append_uuid {
                let uuid = Uuid::new_v4();
                format!("{}-{}", seconds, uuid.hyphenated())
            } else {
                seconds.to_string()
            }
        };

        let key = format!("{}{}.{}", key, filename, self.extension);

        let metadata = metadata_builder.build(&payload);
        let body = payload.into_payload();

        GcsRequest {
            key,
            body,
            finalizers,
            settings: GcsRequestSettings {
                acl: self.acl.clone(),
                content_type: self.content_type.clone(),
                content_encoding: self.content_encoding.clone(),
                storage_class: self.storage_class.clone(),
                headers: self.headers.clone(),
            },
            metadata,
        }
    }
}

impl RequestSettings {
    fn new(config: &GcsSinkConfig) -> crate::Result<Self> {
        let transformer = config.encoding.transformer();
        let (framer, serializer) = config.encoding.build(SinkType::MessageBased)?;
        let encoder = Encoder::<Framer>::new(framer, serializer);
        let acl = config
            .acl
            .map(|acl| HeaderValue::from_str(&to_string(acl)).unwrap());
        let content_type = HeaderValue::from_str(encoder.content_type()).unwrap();
        let content_encoding = config
            .compression
            .content_encoding()
            .map(|ce| HeaderValue::from_str(&to_string(ce)).unwrap());
        let storage_class = config.storage_class.unwrap_or_default();
        let storage_class = HeaderValue::from_str(&to_string(storage_class)).unwrap();
        let metadata = config
            .metadata
            .as_ref()
            .map(|metadata| {
                metadata
                    .iter()
                    .map(make_header)
                    .collect::<Result<Vec<_>, _>>()
            })
            .unwrap_or_else(|| Ok(vec![]))?;
        let extension = config
            .filename_extension
            .clone()
            .unwrap_or_else(|| config.compression.extension().into());
        let time_format = config
            .filename_time_format
            .clone()
            .unwrap_or_else(|| "%s".into());
        let append_uuid = config.filename_append_uuid.unwrap_or(true);
        Ok(Self {
            acl,
            content_type,
            content_encoding,
            storage_class,
            headers: metadata,
            extension,
            time_format,
            append_uuid,
            compression: config.compression,
            encoder: (transformer, encoder),
        })
    }
}

// Make a header pair from a key-value string pair
fn make_header((name, value): (&String, &String)) -> crate::Result<(HeaderName, HeaderValue)> {
    Ok((
        HeaderName::from_bytes(name.as_bytes())?,
        HeaderValue::from_str(value)?,
    ))
}

#[cfg(test)]
mod tests {
    use codecs::encoding::FramingConfig;
    use codecs::{JsonSerializerConfig, NewlineDelimitedEncoderConfig, TextSerializerConfig};
    use futures_util::{future::ready, stream};
    use vector_core::partition::Partitioner;

    use crate::event::LogEvent;
    use crate::test_util::{
        components::{run_and_assert_sink_compliance, SINK_TAGS},
        http::{always_200_response, spawn_blackhole_http_server},
    };

    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<GcsSinkConfig>();
    }

    #[tokio::test]
    async fn component_spec_compliance() {
        let mock_endpoint = spawn_blackhole_http_server(always_200_response).await;

        let context = SinkContext::new_test();

        let tls = TlsSettings::default();
        let client =
            HttpClient::new(tls, context.proxy()).expect("should not fail to create HTTP client");

        let config = default_config((None::<FramingConfig>, JsonSerializerConfig::new()).into());
        let sink = config
            .build_sink(client, mock_endpoint.to_string(), GcpAuthenticator::None)
            .expect("failed to build sink");

        let event = Event::Log(LogEvent::from("simple message"));
        run_and_assert_sink_compliance(sink, stream::once(ready(event)), &SINK_TAGS).await;
    }

    #[test]
    fn gcs_encode_event_apply_rules() {
        crate::test_util::trace_init();

        let message = "hello world".to_string();
        let mut event = LogEvent::from(message);
        event.insert("key", "value");

        let sink_config = GcsSinkConfig {
            key_prefix: Some("key: {{ key }}".into()),
            ..default_config((None::<FramingConfig>, TextSerializerConfig::new()).into())
        };
        let key = sink_config
            .key_partitioner()
            .unwrap()
            .partition(&Event::Log(event))
            .expect("key wasn't provided");

        assert_eq!(key, "key: value");
    }

    fn request_settings(sink_config: &GcsSinkConfig) -> RequestSettings {
        RequestSettings::new(sink_config).expect("Could not create request settings")
    }

    fn build_request(extension: Option<&str>, uuid: bool, compression: Compression) -> GcsRequest {
        let sink_config = GcsSinkConfig {
            key_prefix: Some("key/".into()),
            filename_time_format: Some("date".into()),
            filename_extension: extension.map(Into::into),
            filename_append_uuid: Some(uuid),
            compression,
            ..default_config(
                (
                    Some(NewlineDelimitedEncoderConfig::new()),
                    JsonSerializerConfig::new(),
                )
                    .into(),
            )
        };
        let log = LogEvent::default().into();
        let key = sink_config
            .key_partitioner()
            .unwrap()
            .partition(&log)
            .expect("key wasn't provided");
        let request_settings = request_settings(&sink_config);
        let (metadata, _events) = request_settings.split_input((key, vec![log]));
        request_settings.build_request(metadata, EncodeResult::uncompressed(Bytes::new()))
    }

    #[test]
    fn gcs_build_request() {
        let req = build_request(Some("ext"), false, Compression::None);
        assert_eq!(req.key, "key/date.ext".to_string());

        let req = build_request(None, false, Compression::None);
        assert_eq!(req.key, "key/date.log".to_string());

        let req = build_request(None, false, Compression::gzip_default());
        assert_eq!(req.key, "key/date.log.gz".to_string());

        let req = build_request(None, true, Compression::gzip_default());
        assert_ne!(req.key, "key/date.log.gz".to_string());
    }
}
