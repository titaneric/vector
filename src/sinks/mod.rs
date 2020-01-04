use futures::{Future, Sink};
use snafu::Snafu;

pub mod aws_cloudwatch_logs;
pub mod aws_cloudwatch_metrics;
pub mod aws_kinesis_firehose;
pub mod aws_kinesis_streams;
pub mod aws_s3;
pub mod blackhole;
pub mod clickhouse;
pub mod console;
pub mod datadog_metrics;
pub mod elasticsearch;
pub mod file;
pub mod gcp_pubsub;
pub mod http;
#[cfg(feature = "rdkafka")]
pub mod kafka;
pub mod new_relic_logs;
pub mod prometheus;
pub mod socket;
pub mod splunk_hec;
pub mod statsd;
pub mod util;
pub mod vector;

use crate::Event;

pub type RouterSink = Box<dyn Sink<SinkItem = Event, SinkError = ()> + 'static + Send>;

pub type Healthcheck = Box<dyn Future<Item = (), Error = crate::Error> + Send>;

/// Common build errors
#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("Unable to resolve DNS for {:?}", address))]
    DNSFailure { address: String },
    #[snafu(display("DNS errored {}", source))]
    DNSError { source: crate::dns::DnsError },
    #[snafu(display("Socket address problem: {}", source))]
    SocketAddressError { source: std::io::Error },
    #[snafu(display("URI parse error: {}", source))]
    UriParseError { source: ::http::uri::InvalidUri },
}

/// Common healthcheck errors
#[derive(Debug, Snafu)]
enum HealthcheckError {
    #[snafu(display("Unexpected status: {}", status))]
    UnexpectedStatus { status: ::http::StatusCode },
}
