use std::convert::TryFrom;

use async_trait::async_trait;
use bytes::BytesMut;
use codecs::{encoding::SerializerConfig, JsonSerializerConfig, TextSerializerConfig};
use futures::{stream::BoxStream, FutureExt, StreamExt, TryFutureExt};
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use tokio_util::codec::Encoder as _;
use vector_common::internal_event::{BytesSent, EventsSent};
use vector_core::ByteSizeOf;

use crate::{
    codecs::Encoder,
    config::{
        AcknowledgementsConfig, DataType, GenerateConfig, Input, SinkConfig, SinkContext,
        SinkDescription,
    },
    event::{Event, EventStatus, Finalizable},
    internal_events::{NatsEventSendError, TemplateRenderingError},
    nats::{from_tls_auth_config, NatsAuthConfig, NatsConfigError},
    sinks::util::{
        encoding::{EncodingConfig, EncodingConfigAdapter, EncodingConfigMigrator, Transformer},
        StreamSink,
    },
    template::{Template, TemplateParseError},
    tls::TlsEnableableConfig,
};

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("invalid encoding: {}", source))]
    Encoding {
        source: codecs::encoding::BuildError,
    },
    #[snafu(display("invalid subject template: {}", source))]
    SubjectTemplate { source: TemplateParseError },
    #[snafu(display("NATS Config Error: {}", source))]
    Config { source: NatsConfigError },
    #[snafu(display("NATS Connect Error: {}", source))]
    Connect { source: std::io::Error },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncodingMigrator;

impl EncodingConfigMigrator for EncodingMigrator {
    type Codec = Encoding;

    fn migrate(codec: &Self::Codec) -> SerializerConfig {
        match codec {
            Encoding::Text => TextSerializerConfig::new().into(),
            Encoding::Json => JsonSerializerConfig::new().into(),
        }
    }
}

/**
 * Code dealing with the SinkConfig struct.
 */

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NatsSinkConfig {
    encoding: EncodingConfigAdapter<EncodingConfig<Encoding>, EncodingMigrator>,
    #[serde(
        default,
        deserialize_with = "crate::serde::bool_or_struct",
        skip_serializing_if = "crate::serde::skip_serializing_if_default"
    )]
    pub acknowledgements: AcknowledgementsConfig,
    #[serde(default = "default_name", alias = "name")]
    connection_name: String,
    subject: String,
    url: String,
    tls: Option<TlsEnableableConfig>,
    auth: Option<NatsAuthConfig>,
}

fn default_name() -> String {
    String::from("vector")
}

#[derive(Clone, Copy, Debug, Derivative, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    Text,
    Json,
}

inventory::submit! {
    SinkDescription::new::<NatsSinkConfig>("nats")
}

impl GenerateConfig for NatsSinkConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            acknowledgements: Default::default(),
            auth: None,
            connection_name: "vector".into(),
            encoding: EncodingConfig::from(Encoding::Json).into(),
            subject: "from.vector".into(),
            tls: None,
            url: "nats://127.0.0.1:4222".into(),
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "nats")]
impl SinkConfig for NatsSinkConfig {
    async fn build(
        &self,
        _cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        let sink = NatsSink::new(self.clone()).await?;
        let healthcheck = healthcheck(self.clone()).boxed();
        Ok((super::VectorSink::from_event_streamsink(sink), healthcheck))
    }

    fn input(&self) -> Input {
        Input::new(self.encoding.config().input_type() & DataType::Log)
    }

    fn sink_type(&self) -> &'static str {
        "nats"
    }

    fn acknowledgements(&self) -> Option<&AcknowledgementsConfig> {
        Some(&self.acknowledgements)
    }
}

impl std::convert::TryFrom<&NatsSinkConfig> for nats::asynk::Options {
    type Error = NatsConfigError;

    fn try_from(config: &NatsSinkConfig) -> Result<Self, Self::Error> {
        from_tls_auth_config(&config.connection_name, &config.auth, &config.tls)
    }
}

impl NatsSinkConfig {
    async fn connect(&self) -> Result<nats::asynk::Connection, BuildError> {
        let options: nats::asynk::Options = self.try_into().context(ConfigSnafu)?;

        options.connect(&self.url).await.context(ConnectSnafu)
    }
}

async fn healthcheck(config: NatsSinkConfig) -> crate::Result<()> {
    config.connect().map_ok(|_| ()).map_err(|e| e.into()).await
}

pub struct NatsSink {
    transformer: Transformer,
    encoder: Encoder<()>,
    connection: nats::asynk::Connection,
    subject: Template,
}

impl NatsSink {
    async fn new(config: NatsSinkConfig) -> Result<Self, BuildError> {
        let connection = config.connect().await?;
        let transformer = config.encoding.transformer();
        let serializer = config.encoding.encoding().context(EncodingSnafu)?;
        let encoder = Encoder::<()>::new(serializer);

        Ok(NatsSink {
            connection,
            transformer,
            encoder,
            subject: Template::try_from(config.subject).context(SubjectTemplateSnafu)?,
        })
    }
}

#[async_trait]
impl StreamSink<Event> for NatsSink {
    async fn run(mut self: Box<Self>, mut input: BoxStream<'_, Event>) -> Result<(), ()> {
        while let Some(mut event) = input.next().await {
            let finalizers = event.take_finalizers();

            let subject = match self.subject.render_string(&event) {
                Ok(subject) => subject,
                Err(error) => {
                    emit!(TemplateRenderingError {
                        error,
                        field: Some("subject"),
                        drop_event: true,
                    });
                    finalizers.update_status(EventStatus::Errored);
                    continue;
                }
            };

            self.transformer.transform(&mut event);

            let event_byte_size = event.size_of();

            let mut bytes = BytesMut::new();
            if self.encoder.encode(event, &mut bytes).is_err() {
                // Error is handled by `Encoder`.
                finalizers.update_status(EventStatus::Errored);
                continue;
            }

            match self.connection.publish(&subject, &bytes).await {
                Err(error) => {
                    finalizers.update_status(EventStatus::Errored);

                    emit!(NatsEventSendError { error });
                }
                Ok(_) => {
                    finalizers.update_status(EventStatus::Delivered);

                    emit!(EventsSent {
                        byte_size: event_byte_size,
                        count: 1,
                        output: None
                    });
                    emit!(BytesSent {
                        byte_size: bytes.len(),
                        protocol: "tcp"
                    });
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<NatsSinkConfig>();
    }
}

#[cfg(feature = "nats-integration-tests")]
#[cfg(test)]
mod integration_tests {
    use std::{thread, time::Duration};

    use super::*;
    use crate::nats::{NatsAuthCredentialsFile, NatsAuthNKey, NatsAuthToken, NatsAuthUserPassword};
    use crate::sinks::VectorSink;
    use crate::test_util::{
        components::{run_and_assert_sink_compliance, SINK_TAGS},
        random_lines_with_stream, random_string, trace_init,
    };
    use crate::tls::TlsConfig;

    async fn publish_and_check(conf: NatsSinkConfig) -> Result<(), BuildError> {
        // Publish `N` messages to NATS.
        //
        // Verify with a separate subscriber that the messages were
        // successfully published.

        // Create Sink
        let sink = NatsSink::new(conf.clone()).await?;
        let sink = VectorSink::from_event_streamsink(sink);

        // Establish the consumer subscription.
        let subject = conf.subject.clone();
        let consumer = conf
            .clone()
            .connect()
            .await
            .expect("failed to connect with test consumer");
        let sub = consumer
            .subscribe(&subject)
            .await
            .expect("failed to subscribe with test consumer");

        // Publish events.
        let num_events = 1_000;
        let (input, events) = random_lines_with_stream(100, num_events, None);

        run_and_assert_sink_compliance(sink, events, &SINK_TAGS).await;

        // Unsubscribe from the channel.
        thread::sleep(Duration::from_secs(3));
        sub.drain().await.unwrap();

        let mut output: Vec<String> = Vec::new();
        while let Some(msg) = sub.next().await {
            output.push(String::from_utf8_lossy(&msg.data).to_string())
        }

        assert_eq!(output.len(), input.len());
        assert_eq!(output, input);

        Ok(())
    }

    #[tokio::test]
    async fn nats_no_auth() {
        trace_init();

        let subject = format!("test-{}", random_string(10));
        let url =
            std::env::var("NATS_ADDRESS").unwrap_or_else(|_| String::from("nats://localhost:4222"));

        let conf = NatsSinkConfig {
            acknowledgements: Default::default(),
            encoding: EncodingConfig::from(Encoding::Text).into(),
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url,
            tls: None,
            auth: None,
        };

        let r = publish_and_check(conf).await;
        assert!(
            r.is_ok(),
            "publish_and_check failed, expected Ok(()), got: {:?}",
            r
        );
    }

    #[tokio::test]
    async fn nats_userpass_auth_valid() {
        trace_init();

        let subject = format!("test-{}", random_string(10));
        let url = std::env::var("NATS_USERPASS_ADDRESS")
            .unwrap_or_else(|_| String::from("nats://localhost:4222"));

        let conf = NatsSinkConfig {
            acknowledgements: Default::default(),
            encoding: EncodingConfig::from(Encoding::Text).into(),
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url,
            tls: None,
            auth: Some(NatsAuthConfig::UserPassword {
                user_password: NatsAuthUserPassword {
                    user: "natsuser".into(),
                    password: "natspass".into(),
                },
            }),
        };

        publish_and_check(conf)
            .await
            .expect("publish_and_check failed");
    }

    #[tokio::test]
    async fn nats_userpass_auth_invalid() {
        trace_init();

        let subject = format!("test-{}", random_string(10));
        let url = std::env::var("NATS_USERPASS_ADDRESS")
            .unwrap_or_else(|_| String::from("nats://localhost:4222"));

        let conf = NatsSinkConfig {
            acknowledgements: Default::default(),
            encoding: EncodingConfig::from(Encoding::Text).into(),
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url,
            tls: None,
            auth: Some(NatsAuthConfig::UserPassword {
                user_password: NatsAuthUserPassword {
                    user: "natsuser".into(),
                    password: "wrongpass".into(),
                },
            }),
        };

        let r = publish_and_check(conf).await;
        assert!(
            matches!(r, Err(BuildError::Connect { .. })),
            "publish_and_check failed, expected BuildError::Connect, got: {:?}",
            r
        );
    }

    #[tokio::test]
    async fn nats_token_auth_valid() {
        trace_init();

        let subject = format!("test-{}", random_string(10));
        let url = std::env::var("NATS_TOKEN_ADDRESS")
            .unwrap_or_else(|_| String::from("nats://localhost:4222"));

        let conf = NatsSinkConfig {
            acknowledgements: Default::default(),
            encoding: EncodingConfig::from(Encoding::Text).into(),
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url,
            tls: None,
            auth: Some(NatsAuthConfig::Token {
                token: NatsAuthToken {
                    value: "secret".into(),
                },
            }),
        };

        let r = publish_and_check(conf).await;
        assert!(
            r.is_ok(),
            "publish_and_check failed, expected Ok(()), got: {:?}",
            r
        );
    }

    #[tokio::test]
    async fn nats_token_auth_invalid() {
        trace_init();

        let subject = format!("test-{}", random_string(10));
        let url = std::env::var("NATS_TOKEN_ADDRESS")
            .unwrap_or_else(|_| String::from("nats://localhost:4222"));

        let conf = NatsSinkConfig {
            acknowledgements: Default::default(),
            encoding: EncodingConfig::from(Encoding::Text).into(),
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url,
            tls: None,
            auth: Some(NatsAuthConfig::Token {
                token: NatsAuthToken {
                    value: "wrongsecret".into(),
                },
            }),
        };

        let r = publish_and_check(conf).await;
        assert!(
            matches!(r, Err(BuildError::Connect { .. })),
            "publish_and_check failed, expected BuildError::Connect, got: {:?}",
            r
        );
    }

    #[tokio::test]
    async fn nats_nkey_auth_valid() {
        trace_init();

        let subject = format!("test-{}", random_string(10));
        let url = std::env::var("NATS_NKEY_ADDRESS")
            .unwrap_or_else(|_| String::from("nats://localhost:4222"));

        let conf = NatsSinkConfig {
            acknowledgements: Default::default(),
            encoding: EncodingConfig::from(Encoding::Text).into(),
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url,
            tls: None,
            auth: Some(NatsAuthConfig::Nkey {
                nkey: NatsAuthNKey {
                    nkey: "UD345ZYSUJQD7PNCTWQPINYSO3VH4JBSADBSYUZOBT666DRASFRAWAWT".into(),
                    seed: "SUANIRXEZUROTXNFN3TJYMT27K7ZZVMD46FRIHF6KXKS4KGNVBS57YAFGY".into(),
                },
            }),
        };

        let r = publish_and_check(conf).await;
        assert!(
            r.is_ok(),
            "publish_and_check failed, expected Ok(()), got: {:?}",
            r
        );
    }

    #[tokio::test]
    async fn nats_nkey_auth_invalid() {
        trace_init();

        let subject = format!("test-{}", random_string(10));
        let url = std::env::var("NATS_NKEY_ADDRESS")
            .unwrap_or_else(|_| String::from("nats://localhost:4222"));

        let conf = NatsSinkConfig {
            acknowledgements: Default::default(),
            encoding: EncodingConfig::from(Encoding::Text).into(),
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url,
            tls: None,
            auth: Some(NatsAuthConfig::Nkey {
                nkey: NatsAuthNKey {
                    nkey: "UAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".into(),
                    seed: "SBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB".into(),
                },
            }),
        };

        let r = publish_and_check(conf).await;
        assert!(
            matches!(r, Err(BuildError::Config { .. })),
            "publish_and_check failed, expected BuildError::Config, got: {:?}",
            r
        );
    }

    #[tokio::test]
    async fn nats_tls_valid() {
        trace_init();

        let subject = format!("test-{}", random_string(10));
        let url = std::env::var("NATS_TLS_ADDRESS")
            .unwrap_or_else(|_| String::from("nats://localhost:4222"));

        let conf = NatsSinkConfig {
            acknowledgements: Default::default(),
            encoding: EncodingConfig::from(Encoding::Text).into(),
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url,
            tls: Some(TlsEnableableConfig {
                enabled: Some(true),
                options: TlsConfig {
                    ca_file: Some("tests/data/nats/rootCA.pem".into()),
                    ..Default::default()
                },
            }),
            auth: None,
        };

        let r = publish_and_check(conf).await;
        assert!(
            r.is_ok(),
            "publish_and_check failed, expected Ok(()), got: {:?}",
            r
        );
    }

    #[tokio::test]
    async fn nats_tls_invalid() {
        trace_init();

        let subject = format!("test-{}", random_string(10));
        let url = std::env::var("NATS_TLS_ADDRESS")
            .unwrap_or_else(|_| String::from("nats://localhost:4222"));

        let conf = NatsSinkConfig {
            acknowledgements: Default::default(),
            encoding: EncodingConfig::from(Encoding::Text).into(),
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url,
            tls: None,
            auth: None,
        };

        let r = publish_and_check(conf).await;
        assert!(
            matches!(r, Err(BuildError::Connect { .. })),
            "publish_and_check failed, expected BuildError::Connect, got: {:?}",
            r
        );
    }

    #[tokio::test]
    async fn nats_tls_client_cert_valid() {
        trace_init();

        let subject = format!("test-{}", random_string(10));
        let url = std::env::var("NATS_TLS_CLIENT_CERT_ADDRESS")
            .unwrap_or_else(|_| String::from("nats://localhost:4222"));

        let conf = NatsSinkConfig {
            acknowledgements: Default::default(),
            encoding: EncodingConfig::from(Encoding::Text).into(),
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url,
            tls: Some(TlsEnableableConfig {
                enabled: Some(true),
                options: TlsConfig {
                    ca_file: Some("tests/data/nats/rootCA.pem".into()),
                    crt_file: Some("tests/data/nats/nats-client.pem".into()),
                    key_file: Some("tests/data/nats/nats-client.key".into()),
                    ..Default::default()
                },
            }),
            auth: None,
        };

        let r = publish_and_check(conf).await;
        assert!(
            r.is_ok(),
            "publish_and_check failed, expected Ok(()), got: {:?}",
            r
        );
    }

    #[tokio::test]
    async fn nats_tls_client_cert_invalid() {
        trace_init();

        let subject = format!("test-{}", random_string(10));
        let url = std::env::var("NATS_TLS_CLIENT_CERT_ADDRESS")
            .unwrap_or_else(|_| String::from("nats://localhost:4222"));

        let conf = NatsSinkConfig {
            acknowledgements: Default::default(),
            encoding: EncodingConfig::from(Encoding::Text).into(),
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url,
            tls: Some(TlsEnableableConfig {
                enabled: Some(true),
                options: TlsConfig {
                    ca_file: Some("tests/data/nats/rootCA.pem".into()),
                    ..Default::default()
                },
            }),
            auth: None,
        };

        let r = publish_and_check(conf).await;
        assert!(
            matches!(r, Err(BuildError::Connect { .. })),
            "publish_and_check failed, expected BuildError::Connect, got: {:?}",
            r
        );
    }

    #[tokio::test]
    async fn nats_tls_jwt_auth_valid() {
        trace_init();

        let subject = format!("test-{}", random_string(10));
        let url = std::env::var("NATS_JWT_ADDRESS")
            .unwrap_or_else(|_| String::from("nats://localhost:4222"));

        let conf = NatsSinkConfig {
            acknowledgements: Default::default(),
            encoding: EncodingConfig::from(Encoding::Text).into(),
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url,
            tls: Some(TlsEnableableConfig {
                enabled: Some(true),
                options: TlsConfig {
                    ca_file: Some("tests/data/nats/rootCA.pem".into()),
                    ..Default::default()
                },
            }),
            auth: Some(NatsAuthConfig::CredentialsFile {
                credentials_file: NatsAuthCredentialsFile {
                    path: "tests/data/nats/nats.creds".into(),
                },
            }),
        };

        let r = publish_and_check(conf).await;
        assert!(
            r.is_ok(),
            "publish_and_check failed, expected Ok(()), got: {:?}",
            r
        );
    }

    #[tokio::test]
    async fn nats_tls_jwt_auth_invalid() {
        trace_init();

        let subject = format!("test-{}", random_string(10));
        let url = std::env::var("NATS_JWT_ADDRESS")
            .unwrap_or_else(|_| String::from("nats://localhost:4222"));

        let conf = NatsSinkConfig {
            acknowledgements: Default::default(),
            encoding: EncodingConfig::from(Encoding::Text).into(),
            connection_name: "".to_owned(),
            subject: subject.clone(),
            url,
            tls: Some(TlsEnableableConfig {
                enabled: Some(true),
                options: TlsConfig {
                    ca_file: Some("tests/data/nats/rootCA.pem".into()),
                    ..Default::default()
                },
            }),
            auth: Some(NatsAuthConfig::CredentialsFile {
                credentials_file: NatsAuthCredentialsFile {
                    path: "tests/data/nats/nats-bad.creds".into(),
                },
            }),
        };

        let r = publish_and_check(conf).await;
        assert!(
            matches!(r, Err(BuildError::Connect { .. })),
            "publish_and_check failed, expected BuildError::Connect, got: {:?}",
            r
        );
    }
}
