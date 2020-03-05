mod request;

use crate::{
    dns::Resolver,
    event::{self, Event, LogEvent, Value},
    region::RegionOrEndpoint,
    sinks::util::{
        encoding::{EncodingConfig, EncodingConfiguration},
        retries::{FixedRetryPolicy, RetryLogic},
        rusoto::{self, AwsCredentialsProvider},
        BatchEventsConfig, BatchServiceSink, PartitionBuffer, PartitionInnerBuffer, SinkExt,
        TowerRequestConfig, TowerRequestSettings,
    },
    template::Template,
    topology::config::{DataType, SinkConfig, SinkContext},
};
use bytes::Bytes;
use futures01::{future, stream::iter_ok, sync::oneshot, Async, Future, Poll, Sink};
use lazy_static::lazy_static;
use rusoto_core::{request::BufferedHttpResponse, Region, RusotoError};
use rusoto_logs::{
    CloudWatchLogs, CloudWatchLogsClient, CreateLogGroupError, CreateLogStreamError,
    DescribeLogGroupsRequest, DescribeLogStreamsError, InputLogEvent, PutLogEventsError,
};
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use std::{collections::HashMap, convert::TryInto, fmt};
use tower::{
    buffer::Buffer,
    limit::{
        concurrency::ConcurrencyLimit,
        rate::{Rate, RateLimit},
    },
    retry::Retry,
    timeout::Timeout,
    Service, ServiceBuilder, ServiceExt,
};

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("{}", source))]
    HttpClientError {
        source: rusoto_core::request::TlsError,
    },
    #[snafu(display("{}", source))]
    InvalidCloudwatchCredentials {
        source: rusoto_core::CredentialsError,
    },
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct CloudwatchLogsSinkConfig {
    pub group_name: Template,
    pub stream_name: Template,
    #[serde(flatten)]
    pub region: RegionOrEndpoint,
    pub encoding: EncodingConfig<Encoding>,
    pub create_missing_group: Option<bool>,
    pub create_missing_stream: Option<bool>,
    #[serde(default)]
    pub batch: BatchEventsConfig,
    #[serde(default)]
    pub request: TowerRequestConfig,
    pub assume_role: Option<String>,
}

#[cfg(test)]
fn default_config(e: Encoding) -> CloudwatchLogsSinkConfig {
    CloudwatchLogsSinkConfig {
        group_name: Default::default(),
        stream_name: Default::default(),
        region: Default::default(),
        encoding: e.into(),
        create_missing_group: Default::default(),
        create_missing_stream: Default::default(),
        batch: Default::default(),
        request: Default::default(),
        assume_role: Default::default(),
    }
}

lazy_static! {
    static ref REQUEST_DEFAULTS: TowerRequestConfig = TowerRequestConfig {
        ..Default::default()
    };
}

pub struct CloudwatchLogsSvc {
    client: CloudWatchLogsClient,
    encoding: EncodingConfig<Encoding>,
    stream_name: String,
    group_name: String,
    create_missing_group: bool,
    create_missing_stream: bool,
    token: Option<String>,
    token_rx: Option<oneshot::Receiver<Option<String>>>,
}

type Svc = Buffer<
    ConcurrencyLimit<
        RateLimit<
            Retry<
                FixedRetryPolicy<CloudwatchRetryLogic>,
                Buffer<Timeout<CloudwatchLogsSvc>, Vec<Event>>,
            >,
        >,
    >,
    Vec<Event>,
>;

pub struct CloudwatchLogsPartitionSvc {
    config: CloudwatchLogsSinkConfig,
    clients: HashMap<CloudwatchKey, Svc>,
    request_settings: TowerRequestSettings,
    resolver: Resolver,
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    Text,
    Json,
}

#[derive(Debug)]
pub enum CloudwatchError {
    Put(RusotoError<PutLogEventsError>),
    Describe(RusotoError<DescribeLogStreamsError>),
    CreateStream(RusotoError<CreateLogStreamError>),
    CreateGroup(RusotoError<CreateLogGroupError>),
    NoStreamsFound,
    ServiceDropped,
    MakeService,
}

#[typetag::serde(name = "aws_cloudwatch_logs")]
impl SinkConfig for CloudwatchLogsSinkConfig {
    fn build(&self, cx: SinkContext) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
        let batch = self.batch.unwrap_or(1000, 1);
        let request = self.request.unwrap_with(&REQUEST_DEFAULTS);

        let log_group = self.group_name.clone();
        let log_stream = self.stream_name.clone();

        let svc = ServiceBuilder::new()
            .concurrency_limit(request.in_flight_limit)
            .service(CloudwatchLogsPartitionSvc::new(
                self.clone(),
                cx.resolver(),
            )?);

        let sink = {
            let svc_sink = BatchServiceSink::new(svc, cx.acker())
                .partitioned_batched_with_min(PartitionBuffer::new(Vec::new()), &batch)
                .with_flat_map(move |event| iter_ok(partition(event, &log_group, &log_stream)));
            Box::new(svc_sink)
        };

        let healthcheck = healthcheck(self.clone(), cx.resolver())?;

        Ok((sink, healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "aws_cloudwatch_logs"
    }
}

impl CloudwatchLogsPartitionSvc {
    pub fn new(config: CloudwatchLogsSinkConfig, resolver: Resolver) -> crate::Result<Self> {
        let request_settings = config.request.unwrap_with(&REQUEST_DEFAULTS);

        Ok(Self {
            config,
            clients: HashMap::new(),
            request_settings,
            resolver,
        })
    }
}

impl Service<PartitionInnerBuffer<Vec<Event>, CloudwatchKey>> for CloudwatchLogsPartitionSvc {
    type Response = ();
    type Error = crate::Error;
    type Future = Box<dyn Future<Item = Self::Response, Error = Self::Error> + Send + 'static>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        Ok(().into())
    }

    fn call(&mut self, req: PartitionInnerBuffer<Vec<Event>, CloudwatchKey>) -> Self::Future {
        let (events, key) = req.into_parts();

        let svc = if let Some(svc) = &mut self.clients.get_mut(&key) {
            svc.clone()
        } else {
            let svc = {
                let policy = self.request_settings.retry_policy(CloudwatchRetryLogic);

                let cloudwatch =
                    CloudwatchLogsSvc::new(&self.config, &key, self.resolver.clone()).unwrap();
                let timeout = Timeout::new(cloudwatch, self.request_settings.timeout);

                let buffer = Buffer::new(timeout, 1);
                let retry = Retry::new(policy, buffer);

                let rate = Rate::new(
                    self.request_settings.rate_limit_num,
                    self.request_settings.rate_limit_duration,
                );
                let rate = RateLimit::new(retry, rate);
                let concurrency = ConcurrencyLimit::new(rate, 1);

                Buffer::new(concurrency, 1)
            };

            self.clients.insert(key, svc.clone());
            svc
        };

        let fut = svc
            .ready()
            .map_err(Into::into)
            .and_then(move |mut svc| svc.call(events))
            .map_err(Into::into);

        Box::new(fut)
    }
}

impl CloudwatchLogsSvc {
    pub fn new(
        config: &CloudwatchLogsSinkConfig,
        key: &CloudwatchKey,
        resolver: Resolver,
    ) -> crate::Result<Self> {
        let region = config.region.clone().try_into()?;
        let client = create_client(region, config.assume_role.clone(), resolver)?;

        let group_name = String::from_utf8_lossy(&key.group[..]).into_owned();
        let stream_name = String::from_utf8_lossy(&key.stream[..]).into_owned();

        let create_missing_group = config.create_missing_group.unwrap_or(true);
        let create_missing_stream = config.create_missing_stream.unwrap_or(true);

        Ok(CloudwatchLogsSvc {
            client,
            encoding: config.encoding.clone(),
            stream_name,
            group_name,
            create_missing_group,
            create_missing_stream,
            token: None,
            token_rx: None,
        })
    }

    pub fn encode_log(&self, mut log: LogEvent) -> InputLogEvent {
        let timestamp =
            if let Some(Value::Timestamp(ts)) = log.remove(&event::log_schema().timestamp_key()) {
                ts.timestamp_millis()
            } else {
                chrono::Utc::now().timestamp_millis()
            };

        match self.encoding.codec {
            Encoding::Json => {
                let message = serde_json::to_string(&log).unwrap();
                InputLogEvent { message, timestamp }
            }
            Encoding::Text => {
                let message = log
                    .get(&event::log_schema().message_key())
                    .map(|v| v.to_string_lossy())
                    .unwrap_or_else(|| "".into());
                InputLogEvent { message, timestamp }
            }
        }
    }
}

impl Service<Vec<Event>> for CloudwatchLogsSvc {
    type Response = ();
    type Error = CloudwatchError;
    type Future = request::CloudwatchFuture;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        if let Some(rx) = &mut self.token_rx {
            match rx.poll() {
                Ok(Async::Ready(token)) => {
                    self.token = token;
                    self.token_rx = None;
                    Ok(().into())
                }
                Ok(Async::NotReady) => Ok(Async::NotReady),
                Err(oneshot::Canceled) => {
                    // This case only happens when the `tx` end gets dropped due to an error
                    // in this case we just reset the token and try again.
                    self.token = None;
                    self.token_rx = None;
                    Ok(().into())
                }
            }
        } else {
            Ok(().into())
        }
    }

    fn call(&mut self, req: Vec<Event>) -> Self::Future {
        if self.token_rx.is_none() {
            let events = req
                .into_iter()
                .map(|mut e| {
                    self.encoding.apply_rules(&mut e);
                    e
                })
                .map(|e| e.into_log())
                .map(|e| self.encode_log(e))
                .collect::<Vec<_>>();

            let (tx, rx) = oneshot::channel();
            self.token_rx = Some(rx);

            info!(message = "Sending events.", events = %events.len());
            request::CloudwatchFuture::new(
                self.client.clone(),
                self.stream_name.clone(),
                self.group_name.clone(),
                self.create_missing_group,
                self.create_missing_stream,
                events,
                self.token.take(),
                tx,
            )
        } else {
            panic!("poll_ready was not called; this is a bug!");
        }
    }
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct CloudwatchKey {
    group: Bytes,
    stream: Bytes,
}

fn partition(
    event: Event,
    group: &Template,
    stream: &Template,
) -> Option<PartitionInnerBuffer<Event, CloudwatchKey>> {
    let group = match group.render(&event) {
        Ok(b) => b,
        Err(missing_keys) => {
            warn!(
                message = "keys in group template do not exist on the event; dropping event.",
                ?missing_keys,
                rate_limit_secs = 30
            );
            return None;
        }
    };

    let stream = match stream.render(&event) {
        Ok(b) => b,
        Err(missing_keys) => {
            warn!(
                message = "keys in stream template do not exist on the event; dropping event.",
                ?missing_keys,
                rate_limit_secs = 30
            );
            return None;
        }
    };

    let key = CloudwatchKey { stream, group };

    Some(PartitionInnerBuffer::new(event, key))
}

#[derive(Debug, Snafu)]
enum HealthcheckError {
    #[snafu(display("DescribeLogStreams failed: {}", source))]
    DescribeLogStreamsFailed {
        source: RusotoError<rusoto_logs::DescribeLogGroupsError>,
    },
    #[snafu(display("No log group found"))]
    NoLogGroup,
    #[snafu(display("Unable to extract group name"))]
    GroupNameError,
    #[snafu(display("Group name mismatch: expected {}, found {}", expected, name))]
    GroupNameMismatch { expected: String, name: String },
}

fn healthcheck(
    config: CloudwatchLogsSinkConfig,
    resolver: Resolver,
) -> crate::Result<super::Healthcheck> {
    if config.group_name.is_dynamic() {
        info!("cloudwatch group_name is dynamic; skipping healthcheck.");
        return Ok(Box::new(future::ok(())));
    }

    let group_name = String::from_utf8_lossy(&config.group_name.get_ref()[..]).into_owned();

    let client = create_client(
        config.region.clone().try_into()?,
        config.assume_role,
        resolver,
    )?;

    let request = DescribeLogGroupsRequest {
        limit: Some(1),
        log_group_name_prefix: Some(group_name.clone()),
        ..Default::default()
    };

    let expected_group_name = group_name;

    // This will attempt to find the group name passed in and verify that
    // it matches the one that AWS sends back.
    let fut = client
        .describe_log_groups(request)
        .map_err(|source| HealthcheckError::DescribeLogStreamsFailed { source }.into())
        .and_then(|response| {
            response
                .log_groups
                .ok_or_else(|| HealthcheckError::NoLogGroup.into())
        })
        .and_then(move |groups| {
            if let Some(group) = groups.into_iter().next() {
                if let Some(name) = group.log_group_name {
                    if name == expected_group_name {
                        Ok(())
                    } else {
                        Err(HealthcheckError::GroupNameMismatch {
                            expected: expected_group_name,
                            name,
                        }
                        .into())
                    }
                } else {
                    Err(HealthcheckError::GroupNameError.into())
                }
            } else {
                Err(HealthcheckError::NoLogGroup.into())
            }
        });

    Ok(Box::new(fut))
}

fn create_client(
    region: Region,
    assume_role: Option<String>,
    resolver: Resolver,
) -> crate::Result<CloudWatchLogsClient> {
    let http = rusoto::client(resolver)?;
    let creds = AwsCredentialsProvider::new(&region, assume_role)?;
    Ok(CloudWatchLogsClient::new_with(http, creds, region))
}

#[derive(Debug, Clone)]
struct CloudwatchRetryLogic;

impl RetryLogic for CloudwatchRetryLogic {
    type Error = CloudwatchError;
    type Response = ();

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        match error {
            CloudwatchError::Put(err) => match err {
                RusotoError::Service(PutLogEventsError::ServiceUnavailable(error)) => {
                    error!(message = "put logs service unavailable.", %error);
                    true
                }

                RusotoError::HttpDispatch(error) => {
                    error!(message = "put logs http dispatch.", %error);
                    true
                }

                RusotoError::Unknown(res)
                    if res.status.is_server_error()
                        || res.status == http::StatusCode::TOO_MANY_REQUESTS =>
                {
                    let BufferedHttpResponse { status, body, .. } = res;
                    let body = String::from_utf8_lossy(&body[..]);
                    let body = &body[..body.len().min(50)];

                    error!(message = "put logs http error.", %status, %body);
                    true
                }

                _ => false,
            },

            CloudwatchError::Describe(err) => match err {
                RusotoError::Service(DescribeLogStreamsError::ServiceUnavailable(error)) => {
                    error!(message = "describe streams service unavailable.", %error);
                    true
                }

                RusotoError::Unknown(res)
                    if res.status.is_server_error()
                        || res.status == http::StatusCode::TOO_MANY_REQUESTS =>
                {
                    let BufferedHttpResponse { status, body, .. } = res;
                    let body = String::from_utf8_lossy(&body[..]);
                    let body = &body[..body.len().min(50)];

                    error!(message = "describe streams http error.", %status, %body);
                    true
                }

                RusotoError::HttpDispatch(error) => {
                    error!(message = "describe streams http dispatch.", %error);
                    true
                }

                _ => false,
            },

            CloudwatchError::CreateStream(err) => match err {
                RusotoError::Service(CreateLogStreamError::ServiceUnavailable(error)) => {
                    error!(message = "create stream service unavailable.", %error);
                    true
                }

                RusotoError::Unknown(res)
                    if res.status.is_server_error()
                        || res.status == http::StatusCode::TOO_MANY_REQUESTS =>
                {
                    let BufferedHttpResponse { status, body, .. } = res;
                    let body = String::from_utf8_lossy(&body[..]);
                    let body = &body[..body.len().min(50)];

                    error!(message = "create stream http error.", %status, %body);
                    true
                }

                RusotoError::HttpDispatch(error) => {
                    error!(message = "create stream http dispatch.", %error);
                    true
                }

                _ => false,
            },
            _ => false,
        }
    }
}

impl fmt::Display for CloudwatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CloudwatchError::Put(e) => write!(f, "CloudwatchError::Put: {}", e),
            CloudwatchError::Describe(e) => write!(f, "CloudwatchError::Describe: {}", e),
            CloudwatchError::CreateStream(e) => write!(f, "CloudwatchError::CreateStream: {}", e),
            CloudwatchError::CreateGroup(e) => write!(f, "CloudwatchError::CreateGroup: {}", e),
            CloudwatchError::NoStreamsFound => write!(f, "CloudwatchError: No Streams Found"),
            CloudwatchError::ServiceDropped => write!(
                f,
                "CloudwatchError: The service was dropped while there was a request in flight."
            ),
            CloudwatchError::MakeService => write!(
                f,
                "CloudwatchError: The inner service was unable to be created."
            ),
        }
    }
}

impl std::error::Error for CloudwatchError {}

impl From<RusotoError<PutLogEventsError>> for CloudwatchError {
    fn from(e: RusotoError<PutLogEventsError>) -> Self {
        CloudwatchError::Put(e)
    }
}

impl From<RusotoError<DescribeLogStreamsError>> for CloudwatchError {
    fn from(e: RusotoError<DescribeLogStreamsError>) -> Self {
        CloudwatchError::Describe(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        dns::Resolver,
        event::{self, Event, Value},
        region::RegionOrEndpoint,
        test_util::runtime,
    };
    use std::collections::HashMap;
    use string_cache::DefaultAtom as Atom;

    #[test]
    fn partition_static() {
        let event = Event::from("hello world");
        let stream = Template::from("stream");
        let group = "group".into();

        let (_event, key) = partition(event, &group, &stream).unwrap().into_parts();

        let expected = CloudwatchKey {
            stream: "stream".into(),
            group: "group".into(),
        };

        assert_eq!(key, expected)
    }

    #[test]
    fn partition_event() {
        let mut event = Event::from("hello world");

        event.as_mut_log().insert("log_stream", "stream");

        let stream = Template::from("{{log_stream}}");
        let group = "group".into();

        let (_event, key) = partition(event, &group, &stream).unwrap().into_parts();

        let expected = CloudwatchKey {
            stream: "stream".into(),
            group: "group".into(),
        };

        assert_eq!(key, expected)
    }

    #[test]
    fn partition_event_with_prefix() {
        let mut event = Event::from("hello world");

        event.as_mut_log().insert("log_stream", "stream");

        let stream = Template::from("abcd-{{log_stream}}");
        let group = "group".into();

        let (_event, key) = partition(event, &group, &stream).unwrap().into_parts();

        let expected = CloudwatchKey {
            stream: "abcd-stream".into(),
            group: "group".into(),
        };

        assert_eq!(key, expected)
    }

    #[test]
    fn partition_event_with_postfix() {
        let mut event = Event::from("hello world");

        event.as_mut_log().insert("log_stream", "stream");

        let stream = Template::from("{{log_stream}}-abcd");
        let group = "group".into();

        let (_event, key) = partition(event, &group, &stream).unwrap().into_parts();

        let expected = CloudwatchKey {
            stream: "stream-abcd".into(),
            group: "group".into(),
        };

        assert_eq!(key, expected)
    }

    #[test]
    fn partition_no_key_event() {
        let event = Event::from("hello world");

        let stream = Template::from("{{log_stream}}");
        let group = "group".into();

        let stream_val = partition(event, &group, &stream);

        assert!(stream_val.is_none());
    }

    fn svc(config: CloudwatchLogsSinkConfig) -> CloudwatchLogsSvc {
        let config = CloudwatchLogsSinkConfig {
            region: RegionOrEndpoint::with_endpoint("http://localhost:6000".into()),
            ..config
        };
        let key = CloudwatchKey {
            stream: "stream".into(),
            group: "group".into(),
        };
        let rt = runtime();
        let resolver = Resolver::new(Vec::new(), rt.executor()).unwrap();
        CloudwatchLogsSvc::new(&config, &key, resolver).unwrap()
    }

    #[test]
    fn cloudwatch_encoded_event_retains_timestamp() {
        let mut event = Event::from("hello world").into_log();
        event.insert("key", "value");
        let encoded = svc(default_config(Encoding::Json)).encode_log(event.clone());

        let ts = if let Value::Timestamp(ts) = event[&event::log_schema().timestamp_key()] {
            ts.timestamp_millis()
        } else {
            panic!()
        };

        assert_eq!(encoded.timestamp, ts);
    }

    #[test]
    fn cloudwatch_encode_log_as_json() {
        let config = default_config(Encoding::Json);
        let mut event = Event::from("hello world").into_log();
        event.insert("key", "value");
        let encoded = svc(config).encode_log(event.clone());
        let map: HashMap<Atom, String> = serde_json::from_str(&encoded.message[..]).unwrap();
        assert!(map.get(&event::log_schema().timestamp_key()).is_none());
    }

    #[test]
    fn cloudwatch_encode_log_as_text() {
        let config = default_config(Encoding::Text);
        let mut event = Event::from("hello world").into_log();
        event.insert("key", "value");
        let encoded = svc(config).encode_log(event.clone());
        assert_eq!(encoded.message, "hello world");
    }
}

#[cfg(feature = "cloudwatch-logs-integration-tests")]
#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::{
        region::RegionOrEndpoint,
        runtime::Runtime,
        test_util::{random_lines_with_stream, random_string},
        topology::config::{SinkConfig, SinkContext},
    };
    use futures01::Sink;
    use pretty_assertions::assert_eq;
    use rusoto_core::Region;
    use rusoto_logs::{CloudWatchLogs, CreateLogGroupRequest, GetLogEventsRequest};

    const GROUP_NAME: &'static str = "vector-cw";

    #[test]
    fn cloudwatch_insert_log_event() {
        let mut rt = Runtime::single_threaded().unwrap();
        let resolver = Resolver::new(Vec::new(), rt.executor()).unwrap();

        let stream_name = gen_name();

        let region = Region::Custom {
            name: "localstack".into(),
            endpoint: "http://localhost:6000".into(),
        };
        ensure_group(region.clone());

        let config = CloudwatchLogsSinkConfig {
            stream_name: stream_name.clone().into(),
            group_name: GROUP_NAME.into(),
            region: RegionOrEndpoint::with_endpoint("http://localhost:6000".into()),
            encoding: Encoding::Text.into(),
            create_missing_group: None,
            create_missing_stream: None,
            batch: Default::default(),
            request: Default::default(),
            assume_role: None,
        };

        let (sink, _) = config.build(SinkContext::new_test(rt.executor())).unwrap();

        let timestamp = chrono::Utc::now();

        let (input_lines, events) = random_lines_with_stream(100, 11);

        let pump = sink.send_all(events);
        let (sink, _) = rt.block_on(pump).unwrap();
        // drop the sink so it closes all its connections
        drop(sink);

        let mut request = GetLogEventsRequest::default();
        request.log_stream_name = stream_name.clone().into();
        request.log_group_name = GROUP_NAME.into();
        request.start_time = Some(timestamp.timestamp_millis());

        let client = create_client(region, None, resolver).unwrap();

        let response = rt.block_on(client.get_log_events(request)).unwrap();

        let events = response.events.unwrap();

        let output_lines = events
            .into_iter()
            .map(|e| e.message.unwrap())
            .collect::<Vec<_>>();

        assert_eq!(output_lines, input_lines);
    }

    #[test]
    fn cloudwatch_dynamic_group_and_stream_creation() {
        let mut rt = Runtime::single_threaded().unwrap();
        let resolver = Resolver::new(Vec::new(), rt.executor()).unwrap();

        let group_name = gen_name();
        let stream_name = gen_name();

        let region = Region::Custom {
            name: "localstack".into(),
            endpoint: "http://localhost:6000".into(),
        };

        let config = CloudwatchLogsSinkConfig {
            stream_name: stream_name.clone().into(),
            group_name: group_name.clone().into(),
            region: RegionOrEndpoint::with_endpoint("http://localhost:6000".into()),
            encoding: Encoding::Text.into(),
            create_missing_group: None,
            create_missing_stream: None,
            batch: Default::default(),
            request: Default::default(),
            assume_role: None,
        };

        let (sink, _) = config.build(SinkContext::new_test(rt.executor())).unwrap();

        let timestamp = chrono::Utc::now();

        let (input_lines, events) = random_lines_with_stream(100, 11);

        let pump = sink.send_all(events);
        let (sink, _) = rt.block_on(pump).unwrap();
        // drop the sink so it closes all its connections
        drop(sink);

        let mut request = GetLogEventsRequest::default();
        request.log_stream_name = stream_name.clone().into();
        request.log_group_name = group_name;
        request.start_time = Some(timestamp.timestamp_millis());

        let client = create_client(region, None, resolver).unwrap();

        let response = rt.block_on(client.get_log_events(request)).unwrap();

        let events = response.events.unwrap();

        let output_lines = events
            .into_iter()
            .map(|e| e.message.unwrap())
            .collect::<Vec<_>>();

        assert_eq!(output_lines, input_lines);
    }

    #[test]
    fn cloudwatch_insert_log_event_batched() {
        let mut rt = Runtime::single_threaded().unwrap();
        let resolver = Resolver::new(Vec::new(), rt.executor()).unwrap();

        let group_name = gen_name();
        let stream_name = gen_name();

        let region = Region::Custom {
            name: "localstack".into(),
            endpoint: "http://localhost:6000".into(),
        };
        ensure_group(region.clone());

        let config = CloudwatchLogsSinkConfig {
            stream_name: stream_name.clone().into(),
            group_name: group_name.clone().into(),
            region: RegionOrEndpoint::with_endpoint("http://localhost:6000".into()),
            encoding: Encoding::Text.into(),
            create_missing_group: None,
            create_missing_stream: None,
            batch: BatchEventsConfig {
                timeout_secs: None,
                max_events: Some(2),
            },
            request: Default::default(),
            assume_role: None,
        };

        let (sink, _) = config.build(SinkContext::new_test(rt.executor())).unwrap();

        let timestamp = chrono::Utc::now();

        let (input_lines, events) = random_lines_with_stream(100, 11);

        let pump = sink.send_all(events);
        let (sink, _) = rt.block_on(pump).unwrap();
        // drop the sink so it closes all its connections
        drop(sink);

        let mut request = GetLogEventsRequest::default();
        request.log_stream_name = stream_name.clone().into();
        request.log_group_name = group_name.into();
        request.start_time = Some(timestamp.timestamp_millis());

        let client = create_client(region, None, resolver).unwrap();

        let response = rt.block_on(client.get_log_events(request)).unwrap();

        let events = response.events.unwrap();

        let output_lines = events
            .into_iter()
            .map(|e| e.message.unwrap())
            .collect::<Vec<_>>();

        assert_eq!(output_lines, input_lines);
    }

    #[test]
    fn cloudwatch_insert_log_event_partitioned() {
        let mut rt = Runtime::single_threaded().unwrap();
        let resolver = Resolver::new(Vec::new(), rt.executor()).unwrap();

        let stream_name = gen_name();

        let region = Region::Custom {
            name: "localstack".into(),
            endpoint: "http://localhost:6000".into(),
        };

        let client = create_client(region.clone(), None, resolver).unwrap();
        ensure_group(region);

        let config = CloudwatchLogsSinkConfig {
            group_name: GROUP_NAME.into(),
            stream_name: format!("{}-{{{{key}}}}", stream_name).into(),
            region: RegionOrEndpoint::with_endpoint("http://localhost:6000".into()),
            encoding: Encoding::Text.into(),
            create_missing_group: None,
            create_missing_stream: None,
            batch: Default::default(),
            request: Default::default(),
            assume_role: None,
        };

        let (sink, _) = config.build(SinkContext::new_test(rt.executor())).unwrap();

        let timestamp = chrono::Utc::now();

        let (input_lines, _) = random_lines_with_stream(100, 10);

        let events = input_lines
            .clone()
            .into_iter()
            .enumerate()
            .map(|(i, e)| {
                let mut event = Event::from(e);
                let stream = format!("{}", (i % 2));
                event.as_mut_log().insert("key", stream);
                event
            })
            .collect::<Vec<_>>();

        let pump = sink.send_all(iter_ok(events));
        let (sink, _) = rt.block_on(pump).unwrap();
        // drop the sink so it closes all its connections
        drop(sink);

        let mut request = GetLogEventsRequest::default();
        request.log_stream_name = format!("{}-0", stream_name);
        request.log_group_name = GROUP_NAME.into();
        request.start_time = Some(timestamp.timestamp_millis());

        let response = rt.block_on(client.get_log_events(request)).unwrap();
        let events = response.events.unwrap();
        let output_lines = events
            .into_iter()
            .map(|e| e.message.unwrap())
            .collect::<Vec<_>>();
        let expected_output = input_lines
            .clone()
            .into_iter()
            .enumerate()
            .filter(|(i, _)| i % 2 == 0)
            .map(|(_, e)| e)
            .collect::<Vec<_>>();

        assert_eq!(output_lines, expected_output);

        let mut request = GetLogEventsRequest::default();
        request.log_stream_name = format!("{}-1", stream_name);
        request.log_group_name = GROUP_NAME.into();
        request.start_time = Some(timestamp.timestamp_millis());

        let response = rt.block_on(client.get_log_events(request)).unwrap();
        let events = response.events.unwrap();
        let output_lines = events
            .into_iter()
            .map(|e| e.message.unwrap())
            .collect::<Vec<_>>();
        let expected_output = input_lines
            .clone()
            .into_iter()
            .enumerate()
            .filter(|(i, _)| i % 2 == 1)
            .map(|(_, e)| e)
            .collect::<Vec<_>>();

        assert_eq!(output_lines, expected_output);
    }

    #[test]
    fn cloudwatch_healthcheck() {
        let region = Region::Custom {
            name: "localstack".into(),
            endpoint: "http://localhost:6000".into(),
        };
        ensure_group(region);

        let config = CloudwatchLogsSinkConfig {
            stream_name: "test-stream".into(),
            group_name: GROUP_NAME.into(),
            region: RegionOrEndpoint::with_endpoint("http://localhost:6000".into()),
            encoding: Encoding::Text.into(),
            create_missing_group: None,
            create_missing_stream: None,
            batch: Default::default(),
            request: Default::default(),
            assume_role: None,
        };

        let mut rt = Runtime::single_threaded().unwrap();
        let resolver = Resolver::new(Vec::new(), rt.executor()).unwrap();

        rt.block_on(healthcheck(config, resolver).unwrap()).unwrap();
    }

    fn ensure_group(region: Region) {
        let mut rt = Runtime::single_threaded().unwrap();
        let resolver = Resolver::new(Vec::new(), rt.executor()).unwrap();

        let client = create_client(region, None, resolver).unwrap();

        let req = CreateLogGroupRequest {
            log_group_name: GROUP_NAME.into(),
            ..Default::default()
        };

        let _ = rt.block_on(client.create_log_group(req));
    }

    fn gen_name() -> String {
        format!("test-{}", random_string(10).to_lowercase())
    }
}
