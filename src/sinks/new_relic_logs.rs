use crate::{
    sinks::http::{Encoding, HttpMethod, HttpSinkConfig},
    sinks::util::{BatchConfig, Compression, TowerRequestConfig},
    topology::config::{DataType, SinkConfig, SinkContext, SinkDescription},
};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use snafu::Snafu;

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display(
        "Missing authentication key, must provide either 'license_key' or 'insert_key'"
    ))]
    MissingAuthParam,
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone, Derivative)]
#[serde(rename_all = "snake_case")]
#[derivative(Default)]
pub enum NewRelicLogsRegion {
    #[derivative(Default)]
    Us,
    Eu,
}

#[derive(Deserialize, Serialize, Debug, Default, Clone)]
pub struct NewRelicLogsConfig {
    pub license_key: Option<String>,
    pub insert_key: Option<String>,
    pub region: Option<NewRelicLogsRegion>,

    #[serde(default, flatten)]
    pub batch: BatchConfig,

    #[serde(flatten)]
    pub request: TowerRequestConfig,
}

inventory::submit! {
    SinkDescription::new::<NewRelicLogsConfig>("new_relic_logs")
}

#[typetag::serde(name = "new_relic_logs")]
impl SinkConfig for NewRelicLogsConfig {
    fn build(&self, cx: SinkContext) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
        let http_conf = self.create_config()?;
        http_conf.build(cx)
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "new_relic_logs"
    }
}

impl NewRelicLogsConfig {
    fn create_config(&self) -> crate::Result<HttpSinkConfig> {
        let mut headers: IndexMap<String, String> = IndexMap::new();

        if let Some(license_key) = &self.license_key {
            headers.insert("X-License-Key".to_owned(), license_key.clone());
        } else if let Some(insert_key) = &self.insert_key {
            headers.insert("X-Insert-Key".to_owned(), insert_key.clone());
        } else {
            return Err(Box::new(BuildError::MissingAuthParam));
        }

        let uri = match self.region.as_ref().unwrap_or(&NewRelicLogsRegion::Us) {
            NewRelicLogsRegion::Us => "https://log-api.newrelic.com/log/v1",
            NewRelicLogsRegion::Eu => "https://log-api.eu.newrelic.com/log/v1",
        };

        let batch = BatchConfig {
            // The max request size is 10MiB, so in order to be comfortably
            // within this we batch up to 5MiB.
            batch_size: Some(
                self.batch
                    .batch_size
                    .unwrap_or(bytesize::mib(5u64) as usize),
            ),
            ..self.batch
        };

        let request = TowerRequestConfig {
            // The default throughput ceiling defaults are relatively
            // conservative so we crank them up for New Relic.
            request_in_flight_limit: Some(self.request.request_in_flight_limit.unwrap_or(100)),
            request_rate_limit_num: Some(self.request.request_rate_limit_num.unwrap_or(100)),
            ..self.request
        };

        Ok(HttpSinkConfig {
            uri: uri.to_owned(),
            method: Some(HttpMethod::Post),
            healthcheck_uri: None,
            auth: None,
            headers: Some(headers),
            compression: Some(Compression::None),
            encoding: Encoding::Json,

            batch,
            request,

            tls: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        event::Event,
        runtime::Runtime,
        test_util::{next_addr, shutdown_on_idle},
        topology::config::SinkConfig,
    };
    use bytes::Buf;
    use futures::{stream, sync::mpsc, Future, Sink, Stream};
    use hyper::service::service_fn_ok;
    use hyper::{Body, Request, Response, Server};
    use serde_json::Value;
    use std::io::BufRead;

    #[test]
    fn new_relic_logs_check_config_no_auth() {
        assert_eq!(
            format!(
                "{}",
                NewRelicLogsConfig::default().create_config().unwrap_err()
            ),
            "Missing authentication key, must provide either 'license_key' or 'insert_key'"
                .to_owned(),
        );
    }

    #[test]
    fn new_relic_logs_check_config_defaults() {
        let mut nr_config = NewRelicLogsConfig::default();
        nr_config.license_key = Some("foo".to_owned());
        let http_config = nr_config.create_config().unwrap();

        assert_eq!(http_config.uri, "https://log-api.newrelic.com/log/v1");
        assert_eq!(http_config.method, Some(HttpMethod::Post));
        assert_eq!(http_config.encoding, Encoding::Json);
        assert_eq!(
            http_config.batch.batch_size,
            Some(bytesize::mib(5u64) as usize)
        );
        assert_eq!(http_config.request.request_in_flight_limit, Some(100));
        assert_eq!(http_config.request.request_rate_limit_num, Some(100));
        assert_eq!(
            http_config.headers.unwrap()["X-License-Key"],
            "foo".to_owned()
        );
        assert!(http_config.tls.is_none());
        assert!(http_config.auth.is_none());
    }

    #[test]
    fn new_relic_logs_check_config_custom() {
        let mut nr_config = NewRelicLogsConfig::default();
        nr_config.insert_key = Some("foo".to_owned());
        nr_config.region = Some(NewRelicLogsRegion::Eu);
        nr_config.batch.batch_size = Some(bytesize::mib(8u64) as usize);
        nr_config.request.request_in_flight_limit = Some(12);
        nr_config.request.request_rate_limit_num = Some(24);

        let http_config = nr_config.create_config().unwrap();

        assert_eq!(http_config.uri, "https://log-api.eu.newrelic.com/log/v1");
        assert_eq!(http_config.method, Some(HttpMethod::Post));
        assert_eq!(http_config.encoding, Encoding::Json);
        assert_eq!(
            http_config.batch.batch_size,
            Some(bytesize::mib(8u64) as usize)
        );
        assert_eq!(http_config.request.request_in_flight_limit, Some(12));
        assert_eq!(http_config.request.request_rate_limit_num, Some(24));
        assert_eq!(
            http_config.headers.unwrap()["X-Insert-Key"],
            "foo".to_owned()
        );
        assert!(http_config.tls.is_none());
        assert!(http_config.auth.is_none());
    }

    #[test]
    fn new_relic_logs_check_config_custom_from_toml() {
        let config = r#"
        insert_key = "foo"
        region = "eu"
        batch_size = 8388608
        request_in_flight_limit = 12
        request_rate_limit_num = 24
    "#;
        let nr_config: NewRelicLogsConfig = toml::from_str(&config).unwrap();

        let http_config = nr_config.create_config().unwrap();

        assert_eq!(http_config.uri, "https://log-api.eu.newrelic.com/log/v1");
        assert_eq!(http_config.method, Some(HttpMethod::Post));
        assert_eq!(http_config.encoding, Encoding::Json);
        assert_eq!(
            http_config.batch.batch_size,
            Some(bytesize::mib(8u64) as usize)
        );
        assert_eq!(http_config.request.request_in_flight_limit, Some(12));
        assert_eq!(http_config.request.request_rate_limit_num, Some(24));
        assert_eq!(
            http_config.headers.unwrap()["X-Insert-Key"],
            "foo".to_owned()
        );
        assert!(http_config.tls.is_none());
        assert!(http_config.auth.is_none());
    }

    fn build_test_server(
        addr: &std::net::SocketAddr,
    ) -> (
        mpsc::Receiver<(http::request::Parts, hyper::Chunk)>,
        stream_cancel::Trigger,
        impl Future<Item = (), Error = ()>,
    ) {
        let (tx, rx) = mpsc::channel(100);
        let service = move || {
            let tx = tx.clone();
            service_fn_ok(move |req: Request<Body>| {
                let (parts, body) = req.into_parts();

                let tx = tx.clone();
                tokio::spawn(
                    body.concat2()
                        .map_err(|e| panic!(e))
                        .and_then(|body| tx.send((parts, body)))
                        .map(|_| ())
                        .map_err(|e| panic!(e)),
                );

                Response::new(Body::empty())
            })
        };

        let (trigger, tripwire) = stream_cancel::Tripwire::new();
        let server = Server::bind(addr)
            .serve(service)
            .with_graceful_shutdown(tripwire)
            .map_err(|e| panic!("server error: {}", e));

        (rx, trigger, server)
    }

    #[test]
    fn new_relic_logs_happy_path() {
        let in_addr = next_addr();

        let mut nr_config = NewRelicLogsConfig::default();
        nr_config.license_key = Some("foo".to_owned());
        let mut http_config = nr_config.create_config().unwrap();
        http_config.uri = format!("http://{}/fake_nr", in_addr);

        let mut rt = Runtime::new().unwrap();

        let (sink, _healthcheck) = http_config
            .build(SinkContext::new_test(rt.executor()))
            .unwrap();
        let (rx, trigger, server) = build_test_server(&in_addr);

        let input_lines = (0..100).map(|i| format!("msg {}", i)).collect::<Vec<_>>();
        let events = stream::iter_ok(input_lines.clone().into_iter().map(Event::from));

        let pump = sink.send_all(events);

        rt.spawn(server);

        let _ = rt.block_on(pump).unwrap();
        drop(trigger);

        let output_lines = rx
            .wait()
            .map(Result::unwrap)
            .map(|(parts, body)| {
                assert_eq!(hyper::Method::POST, parts.method);
                assert_eq!("/fake_nr", parts.uri.path());
                assert_eq!(
                    parts
                        .headers
                        .get("X-License-Key")
                        .and_then(|v| v.to_str().ok()),
                    Some("foo")
                );
                body
            })
            .map(hyper::Chunk::reader)
            .flat_map(BufRead::lines)
            .map(Result::unwrap)
            .flat_map(|s| -> Vec<String> {
                let vals: Vec<Value> = serde_json::from_str(&s).unwrap();
                vals.iter()
                    .map(|v| v.get("message").unwrap().as_str().unwrap().to_owned())
                    .collect()
            })
            .collect::<Vec<_>>();

        shutdown_on_idle(rt);

        assert_eq!(input_lines, output_lines);
    }
}
