use crate::{
    buffers::Acker,
    event::metric::{Metric, MetricKind, MetricValue},
    sinks::util::{
        http::{Error as HttpError, HttpRetryLogic, HttpService, Response as HttpResponse},
        retries::FixedRetryPolicy,
        BatchConfig, BatchServiceSink, MetricBuffer, SinkExt,
    },
    topology::config::{DataType, SinkConfig, SinkDescription},
};
use chrono::{DateTime, Utc};
use futures::{Future, Poll};
use http::{uri::InvalidUri, Method, StatusCode, Uri};
use hyper;
use hyper_tls::HttpsConnector;
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use std::collections::HashMap;
use std::time::Duration;
use tower::{Service, ServiceBuilder};

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("Invalid host {:?}: {:?}", host, source))]
    InvalidHost { host: String, source: InvalidUri },
}

#[derive(Clone)]
struct DatadogState {
    last_sent_timestamp: i64,
}

#[derive(Clone)]
struct DatadogSvc {
    config: DatadogConfig,
    state: DatadogState,
    inner: HttpService,
}

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct DatadogConfig {
    pub namespace: String,
    #[serde(default = "default_host")]
    pub host: String,
    pub api_key: String,
    #[serde(default, flatten)]
    pub batch: BatchConfig,

    // Tower Request based configuration
    pub request_in_flight_limit: Option<usize>,
    pub request_timeout_secs: Option<u64>,
    pub request_rate_limit_duration_secs: Option<u64>,
    pub request_rate_limit_num: Option<u64>,
    pub request_retry_attempts: Option<usize>,
    pub request_retry_backoff_secs: Option<u64>,
}

pub fn default_host() -> String {
    String::from("https://api.datadoghq.com")
}

// https://docs.datadoghq.com/api/?lang=bash#post-timeseries-points
#[derive(Debug, Clone, PartialEq, Serialize)]
struct DatadogRequest {
    series: Vec<DatadogMetric>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct DatadogMetric {
    metric: String,
    r#type: DatadogMetricType,
    interval: Option<i64>,
    points: Vec<DatadogPoint>,
    tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DatadogMetricType {
    Gauge,
    Count,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct DatadogPoint(i64, f64);

inventory::submit! {
    SinkDescription::new::<DatadogConfig>("datadog")
}

#[typetag::serde(name = "datadog")]
impl SinkConfig for DatadogConfig {
    fn build(&self, acker: Acker) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
        let sink = DatadogSvc::new(self.clone(), acker)?;
        let healthcheck = DatadogSvc::healthcheck(self.clone())?;
        Ok((sink, healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Metric
    }

    fn sink_type(&self) -> &'static str {
        "datadog"
    }
}

impl DatadogSvc {
    pub fn new(config: DatadogConfig, acker: Acker) -> crate::Result<super::RouterSink> {
        let batch = config.batch.unwrap_or(20, 1);

        let timeout = config.request_timeout_secs.unwrap_or(60);
        let in_flight_limit = config.request_in_flight_limit.unwrap_or(5);
        let rate_limit_duration = config.request_rate_limit_duration_secs.unwrap_or(1);
        let rate_limit_num = config.request_rate_limit_num.unwrap_or(5);
        let retry_attempts = config.request_retry_attempts.unwrap_or(5);
        let retry_backoff_secs = config.request_retry_backoff_secs.unwrap_or(1);

        let policy = FixedRetryPolicy::new(
            retry_attempts,
            Duration::from_secs(retry_backoff_secs),
            HttpRetryLogic,
        );

        let uri = format!("{}/api/v1/series?api_key={}", config.host, config.api_key)
            .parse::<Uri>()
            .context(super::UriParseError)?;

        let http_service = HttpService::new(move |body: Vec<u8>| {
            let mut builder = hyper::Request::builder();
            builder.method(Method::POST);
            builder.uri(uri.clone());

            builder.header("Content-Type", "application/json");
            builder.body(body).unwrap()
        });

        let datadog_http_service = DatadogSvc {
            config,
            state: DatadogState {
                last_sent_timestamp: Utc::now().timestamp(),
            },
            inner: http_service,
        };

        let service = ServiceBuilder::new()
            .concurrency_limit(in_flight_limit)
            .rate_limit(rate_limit_num, Duration::from_secs(rate_limit_duration))
            .retry(policy)
            .timeout(Duration::from_secs(timeout))
            .service(datadog_http_service);

        let sink =
            BatchServiceSink::new(service, acker).batched_with_min(MetricBuffer::new(), &batch);

        Ok(Box::new(sink))
    }

    fn healthcheck(config: DatadogConfig) -> crate::Result<super::Healthcheck> {
        let uri = format!("{}/api/v1/validate?api_key={}", config.host, config.api_key)
            .parse::<Uri>()
            .context(super::UriParseError)?;

        let request = hyper::Request::get(uri).body(hyper::Body::empty()).unwrap();

        let https = HttpsConnector::new(4).expect("TLS initialization failed");
        let client = hyper::Client::builder().build(https);

        let healthcheck = client
            .request(request)
            .map_err(|err| err.into())
            .and_then(|response| match response.status() {
                StatusCode::OK => Ok(()),
                other => Err(super::HealthcheckError::UnexpectedStatus { status: other }.into()),
            });

        Ok(Box::new(healthcheck))
    }
}

impl Service<Vec<Metric>> for DatadogSvc {
    type Response = HttpResponse;
    type Error = HttpError;
    type Future = Box<dyn Future<Item = Self::Response, Error = Self::Error> + Send + 'static>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        self.inner.poll_ready()
    }

    fn call(&mut self, items: Vec<Metric>) -> Self::Future {
        let now = Utc::now().timestamp();
        let interval = now - self.state.last_sent_timestamp;
        self.state.last_sent_timestamp = now;

        let input = encode_events(items, interval, &self.config.namespace);
        let body = serde_json::to_vec(&input).unwrap();

        self.inner.call(body)
    }
}

fn encode_tags(tags: HashMap<String, String>) -> Vec<String> {
    let mut pairs: Vec<_> = tags
        .iter()
        .map(|(name, value)| format!("{}:{}", name, value))
        .collect();
    pairs.sort();
    pairs
}

fn encode_timestamp(timestamp: Option<DateTime<Utc>>) -> i64 {
    if let Some(ts) = timestamp {
        ts.timestamp()
    } else {
        Utc::now().timestamp()
    }
}

fn encode_namespace(namespace: &str, name: &str) -> String {
    if !namespace.is_empty() {
        format!("{}.{}", namespace, name)
    } else {
        name.to_string()
    }
}

fn encode_events(events: Vec<Metric>, interval: i64, namespace: &str) -> DatadogRequest {
    let series: Vec<_> = events
        .into_iter()
        .filter_map(|event| {
            let fullname = encode_namespace(namespace, &event.name);
            let ts = encode_timestamp(event.timestamp);
            let tags = event.tags.clone().map(encode_tags);
            match event.kind {
                MetricKind::Incremental => match event.value {
                    MetricValue::Counter { value } => Some(vec![DatadogMetric {
                        metric: fullname,
                        r#type: DatadogMetricType::Count,
                        interval: Some(interval),
                        points: vec![DatadogPoint(ts, value)],
                        tags,
                    }]),
                    MetricValue::Set { values } => Some(vec![DatadogMetric {
                        metric: fullname,
                        r#type: DatadogMetricType::Gauge,
                        interval: None,
                        points: vec![DatadogPoint(ts, values.len() as f64)],
                        tags: tags.clone(),
                    }]),
                    _ => None,
                },
                MetricKind::Absolute => match event.value {
                    MetricValue::Gauge { value } => Some(vec![DatadogMetric {
                        metric: fullname,
                        r#type: DatadogMetricType::Gauge,
                        interval: None,
                        points: vec![DatadogPoint(ts, value)],
                        tags,
                    }]),
                    _ => None,
                },
            }
        })
        .flatten()
        .collect();

    DatadogRequest { series }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::metric::{Metric, MetricKind, MetricValue};
    use chrono::offset::TimeZone;
    use pretty_assertions::assert_eq;

    fn ts() -> DateTime<Utc> {
        Utc.ymd(2018, 11, 14).and_hms_nano(8, 9, 10, 11)
    }

    fn tags() -> HashMap<String, String> {
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
            encode_tags(tags()),
            vec!["empty_tag:", "normal_tag:value", "true_tag:true"]
        );
    }

    #[test]
    fn test_encode_timestamp() {
        assert_eq!(encode_timestamp(None), Utc::now().timestamp());
        assert_eq!(encode_timestamp(Some(ts())), 1542182950);
    }

    #[test]
    fn encode_counter() {
        let now = Utc::now().timestamp();
        let interval = 60;
        let events = vec![
            Metric {
                name: "total".into(),
                timestamp: None,
                tags: None,
                kind: MetricKind::Incremental,
                value: MetricValue::Counter { value: 1.5 },
            },
            Metric {
                name: "check".into(),
                timestamp: Some(ts()),
                tags: Some(tags()),
                kind: MetricKind::Incremental,
                value: MetricValue::Counter { value: 1.0 },
            },
            Metric {
                name: "unsupported".into(),
                timestamp: Some(ts()),
                tags: Some(tags()),
                kind: MetricKind::Absolute,
                value: MetricValue::Counter { value: 1.0 },
            },
        ];
        let input = encode_events(events, interval, "ns");
        let json = serde_json::to_string(&input).unwrap();

        assert_eq!(
            json,
            format!("{{\"series\":[{{\"metric\":\"ns.total\",\"type\":\"count\",\"interval\":60,\"points\":[[{},1.5]],\"tags\":null}},{{\"metric\":\"ns.check\",\"type\":\"count\",\"interval\":60,\"points\":[[1542182950,1.0]],\"tags\":[\"empty_tag:\",\"normal_tag:value\",\"true_tag:true\"]}}]}}", now)
        );
    }

    #[test]
    fn encode_gauge() {
        let events = vec![
            Metric {
                name: "unsupported".into(),
                timestamp: Some(ts()),
                tags: None,
                kind: MetricKind::Incremental,
                value: MetricValue::Gauge { value: 0.1 },
            },
            Metric {
                name: "volume".into(),
                timestamp: Some(ts()),
                tags: None,
                kind: MetricKind::Absolute,
                value: MetricValue::Gauge { value: -1.1 },
            },
        ];
        let input = encode_events(events, 60, "");
        let json = serde_json::to_string(&input).unwrap();

        assert_eq!(
            json,
            r#"{"series":[{"metric":"volume","type":"gauge","interval":null,"points":[[1542182950,-1.1]],"tags":null}]}"#
        );
    }

    #[test]
    fn encode_set() {
        let events = vec![Metric {
            name: "users".into(),
            timestamp: Some(ts()),
            tags: None,
            kind: MetricKind::Incremental,
            value: MetricValue::Set {
                values: vec!["alice".into(), "bob".into()].into_iter().collect(),
            },
        }];
        let input = encode_events(events, 60, "");
        let json = serde_json::to_string(&input).unwrap();

        assert_eq!(
            json,
            r#"{"series":[{"metric":"users","type":"gauge","interval":null,"points":[[1542182950,2.0]],"tags":null}]}"#
        );
    }
}
