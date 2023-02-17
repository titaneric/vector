//! Functionality supporting both the [sources::amqp] source and `[sinks::amqp]` sink.
use lapin::tcp::{OwnedIdentity, OwnedTLSConfig};
use vector_config::configurable_component;

/// AMQP connection options.
#[configurable_component]
#[derive(Clone, Debug)]
pub(crate) struct AmqpConfig {
    /// URI for the AMQP server.
    ///
    /// The URI has the format of
    /// `amqp://<user>:<password>@<host>:<port>/<vhost>?timeout=<seconds>`.
    ///
    /// The default vhost can be specified by using a value of `%2f`.
    ///
    /// In order to connect over TLS, a scheme of `amqps` can be specified instead i.e.
    /// `amqps://...`. Additional TLS settings, such as client certificate verification, etc, can be
    /// configured under the `tls` section.
    #[configurable(metadata(
        docs::examples = "amqp://user:password@127.0.0.1:5672/%2f?timeout=10",
    ))]
    pub(crate) connection_string: String,

    #[configurable(derived)]
    pub(crate) tls: Option<crate::tls::TlsConfig>,
}

impl Default for AmqpConfig {
    fn default() -> Self {
        Self {
            connection_string: "amqp://127.0.0.1/%2f".to_string(),
            tls: None,
        }
    }
}

impl AmqpConfig {
    pub(crate) async fn connect(
        &self,
    ) -> Result<(lapin::Connection, lapin::Channel), Box<dyn std::error::Error + Send + Sync>> {
        debug!("Connecting to {}.", self.connection_string);
        let addr = self.connection_string.clone();
        let conn = match &self.tls {
            Some(tls) => {
                let cert_chain = if let Some(ca) = &tls.ca_file {
                    Some(tokio::fs::read_to_string(ca.to_owned()).await?)
                } else {
                    None
                };
                let identity = if let Some(identity) = &tls.key_file {
                    let der = tokio::fs::read(identity.to_owned()).await?;
                    Some(OwnedIdentity {
                        der,
                        password: tls
                            .key_pass
                            .as_ref()
                            .map(|s| s.to_string())
                            .unwrap_or_else(String::default),
                    })
                } else {
                    None
                };
                let tls_config = OwnedTLSConfig {
                    identity,
                    cert_chain,
                };
                lapin::Connection::connect_with_config(
                    &addr,
                    lapin::ConnectionProperties::default(),
                    tls_config,
                )
                .await
            }
            None => lapin::Connection::connect(&addr, lapin::ConnectionProperties::default()).await,
        }?;
        let channel = conn.create_channel().await?;
        Ok((conn, channel))
    }
}
