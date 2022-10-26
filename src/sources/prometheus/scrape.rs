use std::collections::HashMap;

use bytes::Bytes;
use futures_util::FutureExt;
use http::{response::Parts, Uri};

use snafu::{ResultExt, Snafu};
use vector_config::configurable_component;
use vector_core::{config::LogNamespace, event::Event};

use super::parser;
use crate::sources::util::http::HttpMethod;
use crate::{
    config::{self, GenerateConfig, Output, SourceConfig, SourceContext},
    http::Auth,
    internal_events::PrometheusParseError,
    sources::{
        self,
        util::http_client::{
            build_url, call, default_scrape_interval_secs, GenericHttpClientInputs,
            HttpClientBuilder, HttpClientContext,
        },
    },
    tls::{TlsConfig, TlsSettings},
    Result,
};

// pulled up, and split over multiple lines, because the long lines trip up rustfmt such that it
// gave up trying to format, but reported no error
static PARSE_ERROR_NO_PATH: &str = "No path is set on the endpoint and we got a parse error,\
                                    did you mean to use /metrics? This behavior changed in version 0.11.";
static NOT_FOUND_NO_PATH: &str = "No path is set on the endpoint and we got a 404,\
                                  did you mean to use /metrics?\
                                  This behavior changed in version 0.11.";

#[derive(Debug, Snafu)]
enum ConfigError {
    #[snafu(display("Cannot set both `endpoints` and `hosts`"))]
    BothEndpointsAndHosts,
}

/// Configuration for the `prometheus_scrape` source.
#[configurable_component(source("prometheus_scrape"))]
#[derive(Clone, Debug)]
pub struct PrometheusScrapeConfig {
    /// Endpoints to scrape metrics from.
    #[serde(alias = "hosts")]
    endpoints: Vec<String>,

    /// The interval between scrapes, in seconds.
    #[serde(default = "default_scrape_interval_secs")]
    scrape_interval_secs: u64,

    /// Overrides the name of the tag used to add the instance to each metric.
    ///
    /// The tag value will be the host/port of the scraped instance.
    ///
    /// By default, `"instance"` is used.
    instance_tag: Option<String>,

    /// Overrides the name of the tag used to add the endpoint to each metric.
    ///
    /// The tag value will be the endpoint of the scraped instance.
    ///
    /// By default, `"endpoint"` is used.
    endpoint_tag: Option<String>,

    /// Controls how tag conflicts are handled if the scraped source has tags that Vector would add.
    ///
    /// If `true`, Vector will not add the new tag if the scraped metric has the tag already. If `false`, Vector will
    /// rename the conflicting tag by prepending `exported_` to the name.
    ///
    /// This matches Prometheus’ `honor_labels` configuration.
    #[serde(default = "crate::serde::default_false")]
    honor_labels: bool,

    /// Custom parameters for the scrape request query string.
    ///
    /// One or more values for the same parameter key can be provided. The parameters provided in this option are
    /// appended to any parameters manually provided in the `endpoints` option. This option is especially useful when
    /// scraping the `/federate` endpoint.
    #[serde(default)]
    query: HashMap<String, Vec<String>>,

    #[configurable(derived)]
    tls: Option<TlsConfig>,

    #[configurable(derived)]
    auth: Option<Auth>,
}

impl GenerateConfig for PrometheusScrapeConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            endpoints: vec!["http://localhost:9090/metrics".to_string()],
            scrape_interval_secs: default_scrape_interval_secs(),
            instance_tag: Some("instance".to_string()),
            endpoint_tag: Some("endpoint".to_string()),
            honor_labels: false,
            query: HashMap::new(),
            tls: None,
            auth: None,
        })
        .unwrap()
    }
}

#[async_trait::async_trait]
impl SourceConfig for PrometheusScrapeConfig {
    async fn build(&self, cx: SourceContext) -> Result<sources::Source> {
        let urls = self
            .endpoints
            .iter()
            .map(|s| s.parse::<Uri>().context(sources::UriParseSnafu))
            .map(|r| r.map(|uri| build_url(&uri, &self.query)))
            .collect::<std::result::Result<Vec<Uri>, sources::BuildError>>()?;
        let tls = TlsSettings::from_options(&self.tls)?;

        let builder = PrometheusScrapeBuilder {
            honor_labels: self.honor_labels,
            instance_tag: self.instance_tag.clone(),
            endpoint_tag: self.endpoint_tag.clone(),
        };

        let inputs = GenericHttpClientInputs {
            urls,
            interval_secs: self.scrape_interval_secs,
            headers: HashMap::new(),
            content_type: "text/plain".to_string(),
            auth: self.auth.clone(),
            tls,
            proxy: cx.proxy.clone(),
            shutdown: cx.shutdown,
        };

        Ok(call(inputs, builder, cx.out, HttpMethod::Get).boxed())
    }

    fn outputs(&self, _global_log_namespace: LogNamespace) -> Vec<Output> {
        vec![Output::default(config::DataType::Metric)]
    }

    fn can_acknowledge(&self) -> bool {
        false
    }
}

// InstanceInfo stores the scraped instance info and the tag to insert into the log event with. It
// is used to join these two pieces of info to avoid storing the instance if instance_tag is not
// configured
#[derive(Clone)]
struct InstanceInfo {
    tag: String,
    instance: String,
    honor_label: bool,
}

// EndpointInfo stores the scraped endpoint info and the tag to insert into the log event with. It
// is used to join these two pieces of info to avoid storing the endpoint if endpoint_tag is not
// configured
#[derive(Clone)]
struct EndpointInfo {
    tag: String,
    endpoint: String,
    honor_label: bool,
}

/// Captures the configuration options required to build request-specific context.
#[derive(Clone)]
struct PrometheusScrapeBuilder {
    honor_labels: bool,
    instance_tag: Option<String>,
    endpoint_tag: Option<String>,
}

impl HttpClientBuilder for PrometheusScrapeBuilder {
    type Context = PrometheusScrapeContext;

    /// Expands the context with the instance info and endpoint info for the current request.
    fn build(&self, url: &Uri) -> Self::Context {
        let instance_info = self.instance_tag.as_ref().map(|tag| {
            let instance = format!(
                "{}:{}",
                url.host().unwrap_or_default(),
                url.port_u16().unwrap_or_else(|| match url.scheme() {
                    Some(scheme) if scheme == &http::uri::Scheme::HTTP => 80,
                    Some(scheme) if scheme == &http::uri::Scheme::HTTPS => 443,
                    _ => 0,
                })
            );
            InstanceInfo {
                tag: tag.to_string(),
                instance,
                honor_label: self.honor_labels,
            }
        });
        let endpoint_info = self.endpoint_tag.as_ref().map(|tag| EndpointInfo {
            tag: tag.to_string(),
            endpoint: url.to_string(),
            honor_label: self.honor_labels,
        });
        PrometheusScrapeContext {
            instance_info,
            endpoint_info,
        }
    }
}

/// Request-specific context required for decoding into events.
struct PrometheusScrapeContext {
    instance_info: Option<InstanceInfo>,
    endpoint_info: Option<EndpointInfo>,
}

impl HttpClientContext for PrometheusScrapeContext {
    /// Parses the Prometheus HTTP response into metric events
    fn on_response(&mut self, url: &Uri, _header: &Parts, body: &Bytes) -> Option<Vec<Event>> {
        let body = String::from_utf8_lossy(body);

        match parser::parse_text(&body) {
            Ok(mut events) => {
                for event in events.iter_mut() {
                    let metric = event.as_mut_metric();
                    if let Some(InstanceInfo {
                        tag,
                        instance,
                        honor_label,
                    }) = &self.instance_info
                    {
                        match (honor_label, metric.tag_value(tag)) {
                            (false, Some(old_instance)) => {
                                metric.insert_tag(format!("exported_{}", tag), old_instance);
                                metric.insert_tag(tag.clone(), instance.clone());
                            }
                            (true, Some(_)) => {}
                            (_, None) => {
                                metric.insert_tag(tag.clone(), instance.clone());
                            }
                        }
                    }
                    if let Some(EndpointInfo {
                        tag,
                        endpoint,
                        honor_label,
                    }) = &self.endpoint_info
                    {
                        match (honor_label, metric.tag_value(tag)) {
                            (false, Some(old_endpoint)) => {
                                metric.insert_tag(format!("exported_{}", tag), old_endpoint);
                                metric.insert_tag(tag.clone(), endpoint.clone());
                            }
                            (true, Some(_)) => {}
                            (_, None) => {
                                metric.insert_tag(tag.clone(), endpoint.clone());
                            }
                        }
                    }
                }
                Some(events)
            }
            Err(error) => {
                if url.path() == "/" {
                    // https://github.com/vectordotdev/vector/pull/3801#issuecomment-700723178
                    warn!(
                        message = PARSE_ERROR_NO_PATH,
                        endpoint = %url,
                    );
                }
                emit!(PrometheusParseError {
                    error,
                    url: url.clone(),
                    body,
                });
                None
            }
        }
    }

    fn on_http_response_error(&self, url: &Uri, header: &Parts) {
        if header.status == hyper::StatusCode::NOT_FOUND && url.path() == "/" {
            // https://github.com/vectordotdev/vector/pull/3801#issuecomment-700723178
            warn!(
                message = NOT_FOUND_NO_PATH,
                endpoint = %url,
            );
        }
    }
}

#[cfg(all(test, feature = "sinks-prometheus"))]
mod test {
    use hyper::{
        service::{make_service_fn, service_fn},
        Body, Client, Response, Server,
    };
    use similar_asserts::assert_eq;
    use tokio::time::{sleep, Duration};
    use warp::Filter;

    use super::*;
    use crate::{
        config,
        sinks::prometheus::exporter::PrometheusExporterConfig,
        test_util::{
            components::{run_and_assert_source_compliance, HTTP_PULL_SOURCE_TAGS},
            next_addr, start_topology, trace_init, wait_for_tcp,
        },
        Error,
    };

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<PrometheusScrapeConfig>();
    }

    #[tokio::test]
    async fn test_prometheus_sets_headers() {
        let in_addr = next_addr();

        let dummy_endpoint = warp::path!("metrics").and(warp::header::exact("Accept", "text/plain")).map(|| {
            r#"
                    promhttp_metric_handler_requests_total{endpoint="http://example.com", instance="localhost:9999", code="200"} 100 1612411516789
                    "#
        });

        tokio::spawn(warp::serve(dummy_endpoint).run(in_addr));
        wait_for_tcp(in_addr).await;

        let config = PrometheusScrapeConfig {
            endpoints: vec![format!("http://{}/metrics", in_addr)],
            scrape_interval_secs: 1,
            instance_tag: Some("instance".to_string()),
            endpoint_tag: Some("endpoint".to_string()),
            honor_labels: true,
            query: HashMap::new(),
            auth: None,
            tls: None,
        };

        let events = run_and_assert_source_compliance(
            config,
            Duration::from_secs(3),
            &HTTP_PULL_SOURCE_TAGS,
        )
        .await;
        assert!(!events.is_empty());
    }

    #[tokio::test]
    async fn test_prometheus_honor_labels() {
        let in_addr = next_addr();

        let dummy_endpoint = warp::path!("metrics").map(|| {
                r#"
                    promhttp_metric_handler_requests_total{endpoint="http://example.com", instance="localhost:9999", code="200"} 100 1612411516789
                    "#
        });

        tokio::spawn(warp::serve(dummy_endpoint).run(in_addr));
        wait_for_tcp(in_addr).await;

        let config = PrometheusScrapeConfig {
            endpoints: vec![format!("http://{}/metrics", in_addr)],
            scrape_interval_secs: 1,
            instance_tag: Some("instance".to_string()),
            endpoint_tag: Some("endpoint".to_string()),
            honor_labels: true,
            query: HashMap::new(),
            auth: None,
            tls: None,
        };

        let events = run_and_assert_source_compliance(
            config,
            Duration::from_secs(3),
            &HTTP_PULL_SOURCE_TAGS,
        )
        .await;
        assert!(!events.is_empty());

        let metrics: Vec<_> = events
            .into_iter()
            .map(|event| event.into_metric())
            .collect();

        for metric in metrics {
            assert_eq!(
                metric.tag_value("instance"),
                Some(String::from("localhost:9999"))
            );
            assert_eq!(
                metric.tag_value("endpoint"),
                Some(String::from("http://example.com"))
            );
            assert_eq!(metric.tag_value("exported_instance"), None,);
            assert_eq!(metric.tag_value("exported_endpoint"), None,);
        }
    }

    #[tokio::test]
    async fn test_prometheus_do_not_honor_labels() {
        let in_addr = next_addr();

        let dummy_endpoint = warp::path!("metrics").map(|| {
                r#"
                    promhttp_metric_handler_requests_total{endpoint="http://example.com", instance="localhost:9999", code="200"} 100 1612411516789
                "#
        });

        tokio::spawn(warp::serve(dummy_endpoint).run(in_addr));
        wait_for_tcp(in_addr).await;

        let config = PrometheusScrapeConfig {
            endpoints: vec![format!("http://{}/metrics", in_addr)],
            scrape_interval_secs: 1,
            instance_tag: Some("instance".to_string()),
            endpoint_tag: Some("endpoint".to_string()),
            honor_labels: false,
            query: HashMap::new(),
            auth: None,
            tls: None,
        };

        let events = run_and_assert_source_compliance(
            config,
            Duration::from_secs(3),
            &HTTP_PULL_SOURCE_TAGS,
        )
        .await;
        assert!(!events.is_empty());

        let metrics: Vec<_> = events
            .into_iter()
            .map(|event| event.into_metric())
            .collect();

        for metric in metrics {
            assert_eq!(
                metric.tag_value("instance"),
                Some(format!("{}:{}", in_addr.ip(), in_addr.port()))
            );
            assert_eq!(
                metric.tag_value("endpoint"),
                Some(format!(
                    "http://{}:{}/metrics",
                    in_addr.ip(),
                    in_addr.port()
                ))
            );
            assert_eq!(
                metric.tag_value("exported_instance"),
                Some(String::from("localhost:9999"))
            );
            assert_eq!(
                metric.tag_value("exported_endpoint"),
                Some(String::from("http://example.com"))
            );
        }
    }

    #[tokio::test]
    async fn test_prometheus_request_query() {
        let in_addr = next_addr();

        let dummy_endpoint = warp::path!("metrics").and(warp::query::raw()).map(|query| {
            format!(
                r#"
                    promhttp_metric_handler_requests_total{{query="{}"}} 100 1612411516789
                "#,
                query
            )
        });

        tokio::spawn(warp::serve(dummy_endpoint).run(in_addr));
        wait_for_tcp(in_addr).await;

        let config = PrometheusScrapeConfig {
            endpoints: vec![format!("http://{}/metrics?key1=val1", in_addr)],
            scrape_interval_secs: 1,
            instance_tag: Some("instance".to_string()),
            endpoint_tag: Some("endpoint".to_string()),
            honor_labels: false,
            query: HashMap::from([
                ("key1".to_string(), vec!["val2".to_string()]),
                (
                    "key2".to_string(),
                    vec!["val1".to_string(), "val2".to_string()],
                ),
            ]),
            auth: None,
            tls: None,
        };

        let events = run_and_assert_source_compliance(
            config,
            Duration::from_secs(3),
            &HTTP_PULL_SOURCE_TAGS,
        )
        .await;
        assert!(!events.is_empty());

        let metrics: Vec<_> = events
            .into_iter()
            .map(|event| event.into_metric())
            .collect();

        let expected = HashMap::from([
            (
                "key1".to_string(),
                vec!["val1".to_string(), "val2".to_string()],
            ),
            (
                "key2".to_string(),
                vec!["val1".to_string(), "val2".to_string()],
            ),
        ]);

        for metric in metrics {
            let query = metric.tag_value("query").expect("query must be tagged");
            let mut got: HashMap<String, Vec<String>> = HashMap::new();
            for (k, v) in url::form_urlencoded::parse(query.as_bytes()) {
                got.entry(k.to_string())
                    .or_insert_with(Vec::new)
                    .push(v.to_string());
            }
            for v in got.values_mut() {
                v.sort();
            }
            assert_eq!(got, expected);
        }
    }

    // Intentially not using assert_source_compliance here because this is a round-trip test which
    // means source and sink will both emit `EventsSent` , triggering multi-emission check.
    #[tokio::test]
    async fn test_prometheus_routing() {
        trace_init();
        let in_addr = next_addr();
        let out_addr = next_addr();

        let make_svc = make_service_fn(|_| async {
            Ok::<_, Error>(service_fn(|_| async {
                Ok::<_, Error>(Response::new(Body::from(
                    r##"
                    # HELP promhttp_metric_handler_requests_total Total number of scrapes by HTTP status code.
                    # TYPE promhttp_metric_handler_requests_total counter
                    promhttp_metric_handler_requests_total{code="200"} 100 1612411516789
                    promhttp_metric_handler_requests_total{code="404"} 7 1612411516789
                    prometheus_remote_storage_samples_in_total 57011636 1612411516789
                    # A histogram, which has a pretty complex representation in the text format:
                    # HELP http_request_duration_seconds A histogram of the request duration.
                    # TYPE http_request_duration_seconds histogram
                    http_request_duration_seconds_bucket{le="0.05"} 24054 1612411516789
                    http_request_duration_seconds_bucket{le="0.1"} 33444 1612411516789
                    http_request_duration_seconds_bucket{le="0.2"} 100392 1612411516789
                    http_request_duration_seconds_bucket{le="0.5"} 129389 1612411516789
                    http_request_duration_seconds_bucket{le="1"} 133988 1612411516789
                    http_request_duration_seconds_bucket{le="+Inf"} 144320 1612411516789
                    http_request_duration_seconds_sum 53423 1612411516789
                    http_request_duration_seconds_count 144320 1612411516789
                    # Finally a summary, which has a complex representation, too:
                    # HELP rpc_duration_seconds A summary of the RPC duration in seconds.
                    # TYPE rpc_duration_seconds summary
                    rpc_duration_seconds{code="200",quantile="0.01"} 3102 1612411516789
                    rpc_duration_seconds{code="200",quantile="0.05"} 3272 1612411516789
                    rpc_duration_seconds{code="200",quantile="0.5"} 4773 1612411516789
                    rpc_duration_seconds{code="200",quantile="0.9"} 9001 1612411516789
                    rpc_duration_seconds{code="200",quantile="0.99"} 76656 1612411516789
                    rpc_duration_seconds_sum{code="200"} 1.7560473e+07 1612411516789
                    rpc_duration_seconds_count{code="200"} 2693 1612411516789
                    "##,
                )))
            }))
        });

        tokio::spawn(async move {
            if let Err(error) = Server::bind(&in_addr).serve(make_svc).await {
                error!(message = "Server error.", %error);
            }
        });
        wait_for_tcp(in_addr).await;

        let mut config = config::Config::builder();
        config.add_source(
            "in",
            PrometheusScrapeConfig {
                endpoints: vec![format!("http://{}", in_addr)],
                instance_tag: None,
                endpoint_tag: None,
                honor_labels: false,
                query: HashMap::new(),
                scrape_interval_secs: 1,
                tls: None,
                auth: None,
            },
        );
        config.add_sink(
            "out",
            &["in"],
            PrometheusExporterConfig {
                address: out_addr,
                auth: None,
                tls: None,
                default_namespace: Some("vector".into()),
                buckets: vec![1.0, 2.0, 4.0],
                quantiles: vec![],
                distributions_as_summaries: false,
                flush_period_secs: Duration::from_secs(3),
                suppress_timestamp: false,
                acknowledgements: Default::default(),
            },
        );

        let (topology, _crash) = start_topology(config.build().unwrap(), false).await;
        sleep(Duration::from_secs(1)).await;

        let response = Client::new()
            .get(format!("http://{}/metrics", out_addr).parse().unwrap())
            .await
            .unwrap();

        assert!(response.status().is_success());
        let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
        let lines = std::str::from_utf8(&body)
            .unwrap()
            .lines()
            .collect::<Vec<_>>();

        assert_eq!(lines, vec![
                "# HELP vector_http_request_duration_seconds http_request_duration_seconds",
                "# TYPE vector_http_request_duration_seconds histogram",
                "vector_http_request_duration_seconds_bucket{le=\"0.05\"} 24054 1612411516789",
                "vector_http_request_duration_seconds_bucket{le=\"0.1\"} 33444 1612411516789",
                "vector_http_request_duration_seconds_bucket{le=\"0.2\"} 100392 1612411516789",
                "vector_http_request_duration_seconds_bucket{le=\"0.5\"} 129389 1612411516789",
                "vector_http_request_duration_seconds_bucket{le=\"1\"} 133988 1612411516789",
                "vector_http_request_duration_seconds_bucket{le=\"+Inf\"} 144320 1612411516789",
                "vector_http_request_duration_seconds_sum 53423 1612411516789",
                "vector_http_request_duration_seconds_count 144320 1612411516789",
                "# HELP vector_prometheus_remote_storage_samples_in_total prometheus_remote_storage_samples_in_total",
                "# TYPE vector_prometheus_remote_storage_samples_in_total gauge",
                "vector_prometheus_remote_storage_samples_in_total 57011636 1612411516789",
                "# HELP vector_promhttp_metric_handler_requests_total promhttp_metric_handler_requests_total",
                "# TYPE vector_promhttp_metric_handler_requests_total counter",
                "vector_promhttp_metric_handler_requests_total{code=\"200\"} 100 1612411516789",
                "vector_promhttp_metric_handler_requests_total{code=\"404\"} 7 1612411516789",
                "# HELP vector_rpc_duration_seconds rpc_duration_seconds",
                "# TYPE vector_rpc_duration_seconds summary",
                "vector_rpc_duration_seconds{code=\"200\",quantile=\"0.01\"} 3102 1612411516789",
                "vector_rpc_duration_seconds{code=\"200\",quantile=\"0.05\"} 3272 1612411516789",
                "vector_rpc_duration_seconds{code=\"200\",quantile=\"0.5\"} 4773 1612411516789",
                "vector_rpc_duration_seconds{code=\"200\",quantile=\"0.9\"} 9001 1612411516789",
                "vector_rpc_duration_seconds{code=\"200\",quantile=\"0.99\"} 76656 1612411516789",
                "vector_rpc_duration_seconds_sum{code=\"200\"} 17560473 1612411516789",
                "vector_rpc_duration_seconds_count{code=\"200\"} 2693 1612411516789",
                ],
            );

        topology.stop().await;
    }
}

#[cfg(all(test, feature = "prometheus-integration-tests"))]
mod integration_tests {
    use tokio::time::Duration;

    use super::*;
    use crate::{
        event::{MetricKind, MetricValue},
        test_util::components::{run_and_assert_source_compliance, HTTP_PULL_SOURCE_TAGS},
    };

    #[tokio::test]
    async fn scrapes_metrics() {
        let config = PrometheusScrapeConfig {
            endpoints: vec!["http://localhost:9090/metrics".into()],
            scrape_interval_secs: 1,
            instance_tag: Some("instance".to_string()),
            endpoint_tag: Some("endpoint".to_string()),
            honor_labels: false,
            query: HashMap::new(),
            auth: None,
            tls: None,
        };

        let events = run_and_assert_source_compliance(
            config,
            Duration::from_secs(3),
            &HTTP_PULL_SOURCE_TAGS,
        )
        .await;
        assert!(!events.is_empty());

        let metrics: Vec<_> = events
            .into_iter()
            .map(|event| event.into_metric())
            .collect();

        let find_metric = |name: &str| {
            metrics
                .iter()
                .find(|metric| metric.name() == name)
                .unwrap_or_else(|| panic!("Missing metric {:?}", name))
        };

        // Sample some well-known metrics
        let build = find_metric("prometheus_build_info");
        assert!(matches!(build.kind(), MetricKind::Absolute));
        assert!(matches!(build.value(), &MetricValue::Gauge { .. }));
        assert!(build.tags().unwrap().contains_key("branch"));
        assert!(build.tags().unwrap().contains_key("version"));
        assert_eq!(
            build.tag_value("instance"),
            Some("localhost:9090".to_string())
        );
        assert_eq!(
            build.tag_value("endpoint"),
            Some("http://localhost:9090/metrics".to_string())
        );

        let queries = find_metric("prometheus_engine_queries");
        assert!(matches!(queries.kind(), MetricKind::Absolute));
        assert!(matches!(queries.value(), &MetricValue::Gauge { .. }));
        assert_eq!(
            queries.tag_value("instance"),
            Some("localhost:9090".to_string())
        );
        assert_eq!(
            queries.tag_value("endpoint"),
            Some("http://localhost:9090/metrics".to_string())
        );

        let go_info = find_metric("go_info");
        assert!(matches!(go_info.kind(), MetricKind::Absolute));
        assert!(matches!(go_info.value(), &MetricValue::Gauge { .. }));
        assert!(go_info.tags().unwrap().contains_key("version"));
        assert_eq!(
            go_info.tag_value("instance"),
            Some("localhost:9090".to_string())
        );
        assert_eq!(
            go_info.tag_value("endpoint"),
            Some("http://localhost:9090/metrics".to_string())
        );
    }
}
