//! Generalized HTTP scrape source.
//! Scrapes an endpoint at an interval, decoding the HTTP responses into events.

use bytes::{Bytes, BytesMut};
use chrono::Utc;
use futures_util::FutureExt;
use http::{response::Parts, Uri};
use snafu::ResultExt;
use std::collections::HashMap;
use tokio_util::codec::Decoder as _;

use crate::{
    codecs::{Decoder, DecodingConfig},
    config::{SourceConfig, SourceContext},
    http::Auth,
    serde::default_decoding,
    serde::default_framing_message_based,
    sources,
    sources::util::{
        http::HttpMethod,
        http_scrape::{
            build_url, default_scrape_interval_secs, http_scrape, GenericHttpScrapeInputs,
            HttpScraperBuilder, HttpScraperContext,
        },
    },
    tls::{TlsConfig, TlsSettings},
    Result,
};
use codecs::{
    decoding::{DeserializerConfig, FramingConfig},
    StreamDecodingError,
};
use vector_config::{configurable_component, NamedComponent};
use vector_core::{
    config::{log_schema, LogNamespace, Output},
    event::Event,
};

/// Configuration for the `http_scrape` source.
#[configurable_component(source("http_scrape"))]
#[derive(Clone, Debug)]
pub struct HttpScrapeConfig {
    /// Endpoint to scrape events from. The full path must be specified.
    /// Example: "http://127.0.0.1:9898/logs"
    pub endpoint: String,

    /// The interval between scrapes, in seconds.
    #[serde(default = "default_scrape_interval_secs")]
    pub scrape_interval_secs: u64,

    /// Custom parameters for the scrape request query string.
    ///
    /// One or more values for the same parameter key can be provided. The parameters provided in this option are
    /// appended to any parameters manually provided in the `endpoint` option.
    #[serde(default)]
    pub query: HashMap<String, Vec<String>>,

    /// Decoder to use on the HTTP responses.
    #[configurable(derived)]
    #[serde(default = "default_decoding")]
    pub decoding: DeserializerConfig,

    /// Framing to use in the decoding.
    #[configurable(derived)]
    #[serde(default = "default_framing_message_based")]
    pub framing: FramingConfig,

    /// Headers to apply to the HTTP requests.
    /// One or more values for the same header can be provided.
    #[serde(default)]
    pub headers: HashMap<String, Vec<String>>,

    /// Specifies the action of the HTTP request.
    #[serde(default = "default_http_method")]
    pub method: HttpMethod,

    /// TLS configuration.
    #[configurable(derived)]
    pub tls: Option<TlsConfig>,

    /// HTTP Authentication.
    #[configurable(derived)]
    pub auth: Option<Auth>,

    /// The namespace to use for logs. This overrides the global setting
    #[serde(default)]
    pub log_namespace: Option<bool>,
}

const fn default_http_method() -> HttpMethod {
    HttpMethod::Get
}

impl Default for HttpScrapeConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://localhost:9898/logs".to_string(),
            query: HashMap::new(),
            scrape_interval_secs: default_scrape_interval_secs(),
            decoding: default_decoding(),
            framing: default_framing_message_based(),
            headers: HashMap::new(),
            method: default_http_method(),
            tls: None,
            auth: None,
            log_namespace: None,
        }
    }
}

impl_generate_config_from_default!(HttpScrapeConfig);

#[async_trait::async_trait]
impl SourceConfig for HttpScrapeConfig {
    async fn build(&self, cx: SourceContext) -> Result<sources::Source> {
        // build the url
        let endpoints = vec![self.endpoint.clone()];
        let urls = endpoints
            .iter()
            .map(|s| s.parse::<Uri>().context(sources::UriParseSnafu))
            .map(|r| r.map(|uri| build_url(&uri, &self.query)))
            .collect::<std::result::Result<Vec<Uri>, sources::BuildError>>()?;

        let tls = TlsSettings::from_options(&self.tls)?;

        let log_namespace = cx.log_namespace(self.log_namespace);

        // build the decoder
        let decoder =
            DecodingConfig::new(self.framing.clone(), self.decoding.clone(), log_namespace).build();

        let content_type = self.decoding.content_type(&self.framing).to_string();

        // the only specific context needed is the codec decoding
        let context = HttpScrapeContext {
            decoder,
            log_namespace,
        };

        let inputs = GenericHttpScrapeInputs {
            urls,
            interval_secs: self.scrape_interval_secs,
            headers: self.headers.clone(),
            content_type,
            auth: self.auth.clone(),
            tls,
            proxy: cx.proxy.clone(),
            shutdown: cx.shutdown,
        };

        Ok(http_scrape(inputs, context, cx.out, self.method).boxed())
    }

    fn outputs(&self, global_log_namespace: LogNamespace) -> Vec<Output> {
        // There is a global and per-source `log_namespace` config. The source config overrides the global setting,
        // and is merged here.
        let log_namespace = global_log_namespace.merge(self.log_namespace);

        let schema_definition = self
            .decoding
            .schema_definition(log_namespace)
            .with_standard_vector_source_metadata();

        vec![Output::default(self.decoding.output_type()).with_schema_definition(schema_definition)]
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}

/// Captures the configuration options required to decode the incoming requests into events.
#[derive(Clone)]
struct HttpScrapeContext {
    decoder: Decoder,
    log_namespace: LogNamespace,
}

impl HttpScrapeContext {
    /// Decode the events from the byte buffer
    fn decode_events(&mut self, buf: &mut BytesMut) -> Vec<Event> {
        let mut events = Vec::new();
        loop {
            match self.decoder.decode_eof(buf) {
                Ok(Some((next, _))) => {
                    events.extend(next.into_iter());
                }
                Ok(None) => break,
                Err(error) => {
                    // Error is logged by `crate::codecs::Decoder`, no further
                    // handling is needed here.
                    if !error.can_continue() {
                        break;
                    }
                    break;
                }
            }
        }
        events
    }

    /// Enriches events with source_type, timestamp
    fn enrich_events(&self, events: &mut Vec<Event>) {
        for event in events {
            match event {
                Event::Log(ref mut log) => {
                    self.log_namespace.insert_vector_metadata(
                        log,
                        log_schema().source_type_key(),
                        "source_type",
                        HttpScrapeConfig::NAME,
                    );
                    self.log_namespace.insert_vector_metadata(
                        log,
                        log_schema().timestamp_key(),
                        "ingest_timestamp",
                        Utc::now(),
                    );
                }
                Event::Metric(ref mut metric) => {
                    metric.insert_tag(
                        log_schema().source_type_key().to_string(),
                        HttpScrapeConfig::NAME.to_string(),
                    );
                }
                Event::Trace(ref mut trace) => {
                    trace.insert(
                        log_schema().source_type_key(),
                        Bytes::from(HttpScrapeConfig::NAME),
                    );
                }
            }
        }
    }
}

impl HttpScraperBuilder for HttpScrapeContext {
    type Context = HttpScrapeContext;

    /// No additional context from request data is needed from this particular scraper.
    fn build(&self, _uri: &Uri) -> Self::Context {
        self.clone()
    }
}

impl HttpScraperContext for HttpScrapeContext {
    /// Decodes the HTTP response body into events per the decoder configured.
    fn on_response(
        &mut self,
        _url: &http::Uri,
        _header: &Parts,
        body: &Bytes,
    ) -> Option<Vec<Event>> {
        // get the body into a byte array
        let mut buf = BytesMut::new();
        let body = String::from_utf8_lossy(body);
        buf.extend_from_slice(body.as_bytes());

        // decode and enrich
        let mut events = self.decode_events(&mut buf);
        self.enrich_events(&mut events);

        Some(events)
    }
}
