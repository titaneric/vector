use crate::{
    config::{log_schema, DataType, SinkConfig, SinkContext, SinkDescription},
    sinks::util::{
        encoding::{EncodingConfig, EncodingConfiguration},
        tcp::TcpSink,
        Encoding, StreamSink, UriSerde,
    },
    tls::{MaybeTlsSettings, TlsSettings},
};
use bytes::Bytes;
use futures01::{stream::iter_ok, Sink};
use serde::{Deserialize, Serialize};
use syslog::{Facility, Formatter3164, LogFormat, Severity};

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct PapertrailConfig {
    endpoint: UriSerde,
    encoding: EncodingConfig<Encoding>,
}

inventory::submit! {
    SinkDescription::new_without_default::<PapertrailConfig>("papertrail")
}

#[typetag::serde(name = "papertrail")]
impl SinkConfig for PapertrailConfig {
    fn build(&self, cx: SinkContext) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        let host = self
            .endpoint
            .host()
            .map(str::to_string)
            .ok_or_else(|| "A host is required for endpoint".to_string())?;
        let port = self
            .endpoint
            .port_u16()
            .ok_or_else(|| "A port is required for endpoint".to_string())?;

        let sink = TcpSink::new(
            host,
            port,
            cx.resolver(),
            MaybeTlsSettings::Tls(TlsSettings::default()),
        );
        let healthcheck = sink.healthcheck();

        let pid = std::process::id();

        let encoding = self.encoding.clone();

        let sink = StreamSink::new(sink, cx.acker())
            .with_flat_map(move |e| iter_ok(encode_event(e, pid, &encoding)));

        Ok((
            super::VectorSink::Futures01Sink(Box::new(sink)),
            healthcheck,
        ))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "papertrail"
    }
}

fn encode_event(
    mut event: crate::Event,
    pid: u32,
    encoding: &EncodingConfig<Encoding>,
) -> Option<Bytes> {
    encoding.apply_rules(&mut event);

    let host = if let Some(host) = event.as_mut_log().remove(log_schema().host_key()) {
        Some(host.to_string_lossy())
    } else {
        None
    };

    let formatter = Formatter3164 {
        facility: Facility::LOG_USER,
        hostname: host,
        process: "vector".into(),
        pid: pid as i32,
    };

    let mut s: Vec<u8> = Vec::new();

    let log = event.into_log();

    let message = match encoding.codec() {
        Encoding::Json => serde_json::to_string(&log).unwrap(),
        Encoding::Text => log
            .get(&log_schema().message_key())
            .map(|v| v.to_string_lossy())
            .unwrap_or_default(),
    };

    formatter
        .format(&mut s, Severity::LOG_INFO, message)
        .unwrap();

    s.push(b'\n');

    Some(Bytes::from(s))
}
