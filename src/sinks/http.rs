use crate::{
    dns::Resolver,
    event::{self, Event},
    sinks::util::{
        http::{https_client, HttpRetryLogic, HttpService},
        tls::{TlsOptions, TlsSettings},
        BatchConfig, Buffer, Compression, SinkExt, TowerRequestConfig,
    },
    topology::config::{DataType, SinkConfig, SinkContext, SinkDescription},
};
use futures::{future, stream::iter_ok, Future, Sink};
use headers::HeaderMapExt;
use http::{
    header::{self, HeaderName, HeaderValue},
    Method, Uri,
};
use hyper::{Body, Request};
use indexmap::IndexMap;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("{}: {}", source, name))]
    InvalidHeaderName {
        name: String,
        source: header::InvalidHeaderName,
    },
    #[snafu(display("{}: {}", source, value))]
    InvalidHeaderValue {
        value: String,
        source: header::InvalidHeaderValue,
    },
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct HttpSinkConfig {
    pub uri: String,
    pub method: Option<HttpMethod>,
    pub healthcheck_uri: Option<String>,
    #[serde(flatten)]
    pub basic_auth: Option<BasicAuth>,
    pub headers: Option<IndexMap<String, String>>,
    pub compression: Option<Compression>,
    pub encoding: Encoding,
    #[serde(default, flatten)]
    pub batch: BatchConfig,
    #[serde(flatten)]
    pub request: TowerRequestConfig,
    pub tls: Option<TlsOptions>,
}

lazy_static! {
    static ref REQUEST_DEFAULTS: TowerRequestConfig = TowerRequestConfig {
        request_in_flight_limit: Some(10),
        request_timeout_secs: Some(30),
        request_rate_limit_num: Some(10),
        ..Default::default()
    };
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone, Derivative)]
#[serde(rename_all = "snake_case")]
#[derivative(Default)]
pub enum HttpMethod {
    #[derivative(Default)]
    Post,
    Put,
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone, Derivative)]
#[serde(rename_all = "snake_case")]
#[derivative(Default)]
pub enum Encoding {
    #[derivative(Default)]
    Text,
    Ndjson,
    Json,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct BasicAuth {
    user: String,
    password: String,
}

inventory::submit! {
    SinkDescription::new::<HttpSinkConfig>("http")
}

#[typetag::serde(name = "http")]
impl SinkConfig for HttpSinkConfig {
    fn build(&self, cx: SinkContext) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
        validate_headers(&self.headers)?;
        let tls = TlsSettings::from_options(&self.tls)?;
        let sink = http(self.clone(), cx.clone(), tls.clone())?;

        match self.healthcheck_uri.clone() {
            Some(healthcheck_uri) => {
                let healthcheck =
                    healthcheck(healthcheck_uri, self.basic_auth.clone(), cx.resolver(), tls)?;
                Ok((sink, healthcheck))
            }
            None => Ok((sink, Box::new(future::ok(())))),
        }
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "http"
    }
}

fn http(
    config: HttpSinkConfig,
    cx: SinkContext,
    tls_settings: TlsSettings,
) -> crate::Result<super::RouterSink> {
    let uri = build_uri(&config.uri)?;

    let gzip = match config.compression.unwrap_or(Compression::None) {
        Compression::None => false,
        Compression::Gzip => true,
    };
    let batch = config.batch.unwrap_or(bytesize::mib(10u64), 1);
    let request = config.request.unwrap_with(&REQUEST_DEFAULTS);

    let encoding = config.encoding.clone();
    let headers = config.headers.clone();
    let basic_auth = config.basic_auth.clone();
    let method = config.method.clone().unwrap_or(HttpMethod::Post);

    let http_service = HttpService::builder(cx.resolver())
        .tls_settings(tls_settings)
        .build(move |mut body: Vec<u8>| {
            let mut builder = hyper::Request::builder();

            let method = match method {
                HttpMethod::Post => Method::POST,
                HttpMethod::Put => Method::PUT,
            };

            builder.method(method);

            builder.uri(uri.clone());

            match encoding {
                Encoding::Text => builder.header("Content-Type", "text/plain"),
                Encoding::Ndjson => builder.header("Content-Type", "application/x-ndjson"),
                Encoding::Json => {
                    body.insert(0, b'[');
                    body.pop(); // remove trailing comma from last record
                    body.push(b']');
                    builder.header("Content-Type", "application/json")
                }
            };

            if gzip {
                builder.header("Content-Encoding", "gzip");
            }

            if let Some(headers) = &headers {
                for (header, value) in headers.iter() {
                    builder.header(header.as_str(), value.as_str());
                }
            }

            let mut request = builder.body(body).unwrap();

            if let Some(auth) = &basic_auth {
                auth.apply(request.headers_mut());
            }

            request
        });

    let encoding = config.encoding;
    let sink = request
        .batch_sink(HttpRetryLogic, http_service, cx.acker())
        .batched_with_min(Buffer::new(gzip), &batch)
        .with_flat_map(move |event| iter_ok(encode_event(event, &encoding)));

    Ok(Box::new(sink))
}

fn healthcheck(
    uri: String,
    auth: Option<BasicAuth>,
    resolver: Resolver,
    tls_settings: TlsSettings,
) -> crate::Result<super::Healthcheck> {
    let uri = build_uri(&uri)?;
    let mut request = Request::head(&uri).body(Body::empty()).unwrap();

    if let Some(auth) = auth {
        auth.apply(request.headers_mut());
    }

    let client = https_client(resolver, tls_settings)?;

    let healthcheck = client
        .request(request)
        .map_err(|err| err.into())
        .and_then(|response| {
            use hyper::StatusCode;

            match response.status() {
                StatusCode::OK => Ok(()),
                status => Err(super::HealthcheckError::UnexpectedStatus { status }.into()),
            }
        });

    Ok(Box::new(healthcheck))
}

impl BasicAuth {
    fn apply(&self, header_map: &mut http::header::HeaderMap) {
        let auth = headers::Authorization::basic(&self.user, &self.password);
        header_map.typed_insert(auth)
    }
}

fn validate_headers(headers: &Option<IndexMap<String, String>>) -> crate::Result<()> {
    if let Some(map) = headers {
        for (name, value) in map {
            HeaderName::from_bytes(name.as_bytes()).with_context(|| InvalidHeaderName { name })?;
            HeaderValue::from_bytes(value.as_bytes())
                .with_context(|| InvalidHeaderValue { value })?;
        }
    }
    Ok(())
}

fn build_uri(raw: &str) -> crate::Result<Uri> {
    let base: Uri = raw.parse().context(super::UriParseError)?;
    Ok(Uri::builder()
        .scheme(base.scheme_str().unwrap_or("http"))
        .authority(
            base.authority_part()
                .map(|a| a.as_str())
                .unwrap_or("127.0.0.1"),
        )
        .path_and_query(base.path_and_query().map(|pq| pq.as_str()).unwrap_or(""))
        .build()
        .expect("bug building uri"))
}

fn encode_event(event: Event, encoding: &Encoding) -> Option<Vec<u8>> {
    let event = event.into_log();

    let body = match encoding {
        Encoding::Text => {
            if let Some(v) = event.get(&event::MESSAGE) {
                let mut b = v.to_string_lossy().into_bytes();
                b.push(b'\n');
                b
            } else {
                warn!(
                    message = "Event missing the message key; Dropping event.",
                    rate_limit_secs = 30,
                );
                return None;
            }
        }

        Encoding::Ndjson => {
            let mut b = serde_json::to_vec(&event.unflatten())
                .map_err(|e| panic!("Unable to encode into JSON: {}", e))
                .ok()?;
            b.push(b'\n');
            b
        }

        Encoding::Json => {
            let mut b = serde_json::to_vec(&event.unflatten())
                .map_err(|e| panic!("Unable to encode into JSON: {}", e))
                .ok()?;
            b.push(b',');
            b
        }
    };

    Some(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        assert_downcast_matches,
        runtime::Runtime,
        sinks::http::HttpSinkConfig,
        test_util::{next_addr, random_lines_with_stream, shutdown_on_idle},
        topology::config::SinkContext,
    };
    use bytes::Buf;
    use futures::{sync::mpsc, Future, Sink, Stream};
    use headers::{Authorization, HeaderMapExt};
    use hyper::service::service_fn_ok;
    use hyper::{Body, Request, Response, Server};
    use serde::Deserialize;
    use std::io::{BufRead, BufReader};

    #[test]
    fn http_encode_event_text() {
        let encoding = Encoding::Text;
        let event = Event::from("hello world");

        let bytes = encode_event(event, &encoding).unwrap();

        assert_eq!(bytes, Vec::from(&"hello world\n"[..]));
    }

    #[test]
    fn http_encode_event_json() {
        let encoding = Encoding::Ndjson;
        let event = Event::from("hello world");

        let bytes = encode_event(event, &encoding).unwrap();

        #[derive(Deserialize, Debug)]
        #[serde(deny_unknown_fields)]
        struct ExpectedEvent {
            message: String,
            timestamp: chrono::DateTime<chrono::Utc>,
        }

        let output = serde_json::from_slice::<ExpectedEvent>(&bytes[..]).unwrap();

        assert_eq!(output.message, "hello world".to_string());
    }

    #[test]
    fn http_validates_normal_headers() {
        let config = r#"
        uri = "http://$IN_ADDR/frames"
        encoding = "text"
        [headers]
        Auth = "token:thing_and-stuff"
        X-Custom-Nonsense = "_%_{}_-_&_._`_|_~_!_#_&_$_"
        "#;
        let config: HttpSinkConfig = toml::from_str(&config).unwrap();

        assert!(super::validate_headers(&config.headers).is_ok());
    }

    #[test]
    fn http_catches_bad_header_names() {
        let config = r#"
        uri = "http://$IN_ADDR/frames"
        encoding = "text"
        [headers]
        "\u0001" = "bad"
        "#;
        let config: HttpSinkConfig = toml::from_str(&config).unwrap();

        assert_downcast_matches!(
            super::validate_headers(&config.headers).unwrap_err(),
            BuildError,
            BuildError::InvalidHeaderName{..}
        );
    }

    #[test]
    fn http_happy_path_post() {
        let num_lines = 1000;

        let in_addr = next_addr();

        let config = r#"
        uri = "http://$IN_ADDR/frames"
        user = "waldo"
        compression = "gzip"
        password = "hunter2"
        encoding = "ndjson"
    "#
        .replace("$IN_ADDR", &format!("{}", in_addr));
        let config: HttpSinkConfig = toml::from_str(&config).unwrap();

        let mut rt = Runtime::new().unwrap();
        let cx = SinkContext::new_test(rt.executor());

        let sink = http(config, cx, TlsSettings::default()).unwrap();
        let (rx, trigger, server) = build_test_server(&in_addr);

        let (input_lines, events) = random_lines_with_stream(100, num_lines);
        let pump = sink.send_all(events);

        rt.spawn(server);

        let _ = rt.block_on(pump).unwrap();
        drop(trigger);

        let output_lines = rx
            .wait()
            .map(Result::unwrap)
            .map(|(parts, body)| {
                assert_eq!(hyper::Method::POST, parts.method);
                assert_eq!("/frames", parts.uri.path());
                assert_eq!(
                    Some(Authorization::basic("waldo", "hunter2")),
                    parts.headers.typed_get()
                );
                body
            })
            .map(hyper::Chunk::reader)
            .map(flate2::read::GzDecoder::new)
            .map(BufReader::new)
            .flat_map(BufRead::lines)
            .map(Result::unwrap)
            .map(|s| {
                let val: serde_json::Value = serde_json::from_str(&s).unwrap();
                val.get("message").unwrap().as_str().unwrap().to_owned()
            })
            .collect::<Vec<_>>();

        shutdown_on_idle(rt);

        assert_eq!(num_lines, output_lines.len());
        assert_eq!(input_lines, output_lines);
    }

    #[test]
    fn http_happy_path_put() {
        let num_lines = 1000;

        let in_addr = next_addr();

        let config = r#"
        uri = "http://$IN_ADDR/frames"
        method = "put"
        user = "waldo"
        compression = "gzip"
        password = "hunter2"
        encoding = "ndjson"
    "#
        .replace("$IN_ADDR", &format!("{}", in_addr));
        let config: HttpSinkConfig = toml::from_str(&config).unwrap();

        let mut rt = Runtime::new().unwrap();
        let cx = SinkContext::new_test(rt.executor());

        let sink = http(config, cx, TlsSettings::default()).unwrap();
        let (rx, trigger, server) = build_test_server(&in_addr);

        let (input_lines, events) = random_lines_with_stream(100, num_lines);
        let pump = sink.send_all(events);

        rt.spawn(server);

        let _ = rt.block_on(pump).unwrap();
        drop(trigger);

        let output_lines = rx
            .wait()
            .map(Result::unwrap)
            .map(|(parts, body)| {
                assert_eq!(hyper::Method::PUT, parts.method);
                assert_eq!("/frames", parts.uri.path());
                assert_eq!(
                    Some(Authorization::basic("waldo", "hunter2")),
                    parts.headers.typed_get()
                );
                body
            })
            .map(hyper::Chunk::reader)
            .map(flate2::read::GzDecoder::new)
            .map(BufReader::new)
            .flat_map(BufRead::lines)
            .map(Result::unwrap)
            .map(|s| {
                let val: serde_json::Value = serde_json::from_str(&s).unwrap();
                val.get("message").unwrap().as_str().unwrap().to_owned()
            })
            .collect::<Vec<_>>();

        shutdown_on_idle(rt);

        assert_eq!(num_lines, output_lines.len());
        assert_eq!(input_lines, output_lines);
    }

    #[test]
    fn http_passes_custom_headers() {
        let num_lines = 1000;

        let in_addr = next_addr();

        let config = r#"
        uri = "http://$IN_ADDR/frames"
        encoding = "ndjson"
        compression = "gzip"
        [headers]
        foo = "bar"
        baz = "quux"
    "#
        .replace("$IN_ADDR", &format!("{}", in_addr));
        let config: HttpSinkConfig = toml::from_str(&config).unwrap();

        let mut rt = Runtime::new().unwrap();
        let cx = SinkContext::new_test(rt.executor());

        let sink = http(config, cx, TlsSettings::default()).unwrap();
        let (rx, trigger, server) = build_test_server(&in_addr);

        let (input_lines, events) = random_lines_with_stream(100, num_lines);
        let pump = sink.send_all(events);

        rt.spawn(server);

        let _ = rt.block_on(pump).unwrap();
        drop(trigger);

        let output_lines = rx
            .wait()
            .map(Result::unwrap)
            .map(|(parts, body)| {
                assert_eq!(hyper::Method::POST, parts.method);
                assert_eq!("/frames", parts.uri.path());
                assert_eq!(
                    Some("bar"),
                    parts.headers.get("foo").map(|v| v.to_str().unwrap())
                );
                assert_eq!(
                    Some("quux"),
                    parts.headers.get("baz").map(|v| v.to_str().unwrap())
                );
                body
            })
            .map(hyper::Chunk::reader)
            .map(flate2::read::GzDecoder::new)
            .map(BufReader::new)
            .flat_map(BufRead::lines)
            .map(Result::unwrap)
            .map(|s| {
                let val: serde_json::Value = serde_json::from_str(&s).unwrap();
                val.get("message").unwrap().as_str().unwrap().to_owned()
            })
            .collect::<Vec<_>>();

        shutdown_on_idle(rt);

        assert_eq!(num_lines, output_lines.len());
        assert_eq!(input_lines, output_lines);
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
}
