use super::{healthcheck_response, GcpAuthConfig, GcpCredentials, Scope};
use crate::{
    event::Event,
    sinks::{
        util::{
            encoding::{
                skip_serializing_if_default, EncodingConfigWithDefault, EncodingConfiguration,
            },
            http::{BatchedHttpSink, HttpClient, HttpSink},
            BatchBytesConfig, BoxedRawValue, JsonArrayBuffer, TowerRequestConfig,
        },
        Healthcheck, RouterSink, UriParseError,
    },
    tls::{TlsOptions, TlsSettings},
    topology::config::{DataType, SinkConfig, SinkContext, SinkDescription},
};
use futures01::Future;
use http::Uri;
use hyper::{Body, Method, Request};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use snafu::{ResultExt, Snafu};
use tower::Service;

#[derive(Debug, Snafu)]
enum HealthcheckError {
    #[snafu(display("Configured topic not found"))]
    TopicNotFound,
}

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct PubsubConfig {
    pub project: String,
    pub topic: String,
    pub emulator_host: Option<String>,
    #[serde(flatten)]
    pub auth: GcpAuthConfig,

    #[serde(default)]
    pub batch: BatchBytesConfig,
    #[serde(default)]
    pub request: TowerRequestConfig,
    #[serde(skip_serializing_if = "skip_serializing_if_default", default)]
    pub encoding: EncodingConfigWithDefault<Encoding>,

    pub tls: Option<TlsOptions>,
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone, Derivative)]
#[serde(rename_all = "snake_case")]
#[derivative(Default)]
pub enum Encoding {
    #[derivative(Default)]
    Default,
}

inventory::submit! {
    SinkDescription::new::<PubsubConfig>("gcp_pubsub")
}

#[typetag::serde(name = "gcp_pubsub")]
impl SinkConfig for PubsubConfig {
    fn build(&self, cx: SinkContext) -> crate::Result<(RouterSink, Healthcheck)> {
        let sink = PubsubSink::from_config(self)?;
        let batch_settings = self.batch.unwrap_or(bytesize::mib(10u64), 1);
        let request_settings = self.request.unwrap_with(&Default::default());
        let tls_settings = TlsSettings::from_options(&self.tls)?;

        let healthcheck = sink.healthcheck(&cx, &tls_settings)?;

        let sink = BatchedHttpSink::new(
            sink,
            JsonArrayBuffer::default(),
            request_settings,
            batch_settings,
            Some(tls_settings),
            &cx,
        );

        Ok((Box::new(sink), healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "gcp_pubsub"
    }
}

struct PubsubSink {
    api_key: Option<String>,
    creds: Option<GcpCredentials>,
    uri_base: String,
    encoding: EncodingConfigWithDefault<Encoding>,
}

impl PubsubSink {
    fn from_config(config: &PubsubConfig) -> crate::Result<Self> {
        // We only need to load the credentials if we are not targetting an emulator.
        let creds = match config.emulator_host {
            None => config.auth.make_credentials(Scope::PubSub)?,
            Some(_) => None,
        };

        let uri_base = match config.emulator_host.as_ref() {
            Some(host) => format!("http://{}", host),
            None => "https://pubsub.googleapis.com".into(),
        };
        let uri_base = format!(
            "{}/v1/projects/{}/topics/{}",
            uri_base, config.project, config.topic,
        );

        Ok(Self {
            api_key: config.auth.api_key.clone(),
            encoding: config.encoding.clone(),
            creds,
            uri_base,
        })
    }

    fn healthcheck(&self, cx: &SinkContext, tls: &TlsSettings) -> crate::Result<Healthcheck> {
        let uri = self.uri("")?;
        let mut request = Request::get(uri).body(Body::empty()).unwrap();
        if let Some(creds) = self.creds.as_ref() {
            creds.apply(&mut request);
        }

        let mut client = HttpClient::new(cx.resolver(), tls.clone())?;
        let creds = self.creds.clone();
        let healthcheck = client
            .call(request)
            .map_err(Into::into)
            .and_then(healthcheck_response(
                creds,
                HealthcheckError::TopicNotFound.into(),
            ));
        Ok(Box::new(healthcheck))
    }

    fn uri(&self, suffix: &str) -> crate::Result<Uri> {
        let mut uri = format!("{}{}", self.uri_base, suffix);
        if let Some(key) = &self.api_key {
            uri = format!("{}?key={}", uri, key);
        }
        uri.parse::<Uri>()
            .context(UriParseError)
            .map_err(Into::into)
    }
}

impl HttpSink for PubsubSink {
    type Input = Value;
    type Output = Vec<BoxedRawValue>;

    fn encode_event(&self, mut event: Event) -> Option<Self::Input> {
        self.encoding.apply_rules(&mut event);
        // Each event needs to be base64 encoded, and put into a JSON object
        // as the `data` item.
        let json = serde_json::to_string(&event.into_log()).unwrap();
        Some(json!({ "data": base64::encode(&json) }))
    }

    fn build_request(&self, events: Self::Output) -> http::Request<Vec<u8>> {
        let body = json!({ "messages": events });
        let body = serde_json::to_vec(&body).unwrap();

        let mut builder = hyper::Request::builder();
        builder.method(Method::POST);
        builder.uri(self.uri(":publish").unwrap());
        builder.header("Content-Type", "application/json");

        let mut request = builder.body(body).unwrap();
        if let Some(creds) = &self.creds {
            creds.apply(&mut request);
        }

        request
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::runtime;

    #[test]
    fn fails_missing_creds() {
        let config: PubsubConfig = toml::from_str(
            r#"
           project = "project"
           topic = "topic"
        "#,
        )
        .unwrap();
        if config
            .build(SinkContext::new_test(runtime().executor()))
            .is_ok()
        {
            panic!("config.build failed to error");
        }
    }
}

#[cfg(test)]
#[cfg(feature = "gcp-pubsub-integration-tests")]
mod integration_tests {
    use super::*;
    use crate::{
        runtime::Runtime,
        test_util::{block_on, random_events_with_stream, random_string},
    };
    use futures01::Sink;
    use reqwest::{Client, Method, Response};
    use serde_json::{json, Value};

    const EMULATOR_HOST: &str = "localhost:8681";
    const PROJECT: &str = "testproject";

    fn config(topic: &str) -> PubsubConfig {
        PubsubConfig {
            emulator_host: Some(EMULATOR_HOST.into()),
            project: PROJECT.into(),
            topic: topic.into(),
            ..Default::default()
        }
    }

    fn config_build(
        rt: &Runtime,
        topic: &str,
    ) -> (crate::sinks::RouterSink, crate::sinks::Healthcheck) {
        let cx = SinkContext::new_test(rt.executor());
        config(topic).build(cx).expect("Building sink failed")
    }

    #[test]
    fn publish_events() {
        crate::test_util::trace_init();

        let rt = Runtime::new().unwrap();
        let (topic, subscription) = create_topic_subscription();
        let (sink, healthcheck) = config_build(&rt, &topic);

        block_on(healthcheck).expect("Health check failed");

        let (input, events) = random_events_with_stream(100, 100);

        let pump = sink.send_all(events);
        let _ = block_on(pump).expect("Sending events failed");

        let response = pull_messages(&subscription, 1000);
        let messages = response
            .receivedMessages
            .as_ref()
            .expect("Response is missing messages");
        assert_eq!(input.len(), messages.len());
        for i in 0..input.len() {
            let data = messages[i].message.decode_data();
            let data = serde_json::to_value(data).unwrap();
            let expected = serde_json::to_value(input[i].as_log().all_fields()).unwrap();
            assert_eq!(data, expected);
        }
    }

    #[test]
    fn checks_for_valid_topic() {
        let rt = Runtime::new().unwrap();
        let (topic, _subscription) = create_topic_subscription();
        let topic = format!("BAD{}", topic);
        let (_sink, healthcheck) = config_build(&rt, &topic);
        block_on(healthcheck).expect_err("Health check did not fail");
    }

    fn create_topic_subscription() -> (String, String) {
        let topic = format!("topic-{}", random_string(10));
        let subscription = format!("subscription-{}", random_string(10));
        request(Method::PUT, &format!("topics/{}", topic), json!({}))
            .json::<Value>()
            .expect("Creating new topic failed");
        request(
            Method::PUT,
            &format!("subscriptions/{}", subscription),
            json!({ "topic": format!("projects/{}/topics/{}", PROJECT, topic) }),
        )
        .json::<Value>()
        .expect("Creating new subscription failed");
        (topic, subscription)
    }

    fn request(method: Method, path: &str, json: Value) -> Response {
        let url = format!("http://{}/v1/projects/{}/{}", EMULATOR_HOST, PROJECT, path);
        Client::new()
            .request(method.clone(), &url)
            .json(&json)
            .send()
            .expect(&format!("Sending {} request to {} failed", method, url))
    }

    fn pull_messages(subscription: &str, count: usize) -> PullResponse {
        request(
            Method::POST,
            &format!("subscriptions/{}:pull", subscription),
            json!({
                "returnImmediately": true,
                "maxMessages": count
            }),
        )
        .json::<PullResponse>()
        .expect("Extracting pull data failed")
    }

    #[derive(Debug, Deserialize)]
    #[allow(non_snake_case)]
    struct PullResponse {
        receivedMessages: Option<Vec<PullMessageOuter>>,
    }

    #[derive(Debug, Deserialize)]
    #[allow(non_snake_case)]
    struct PullMessageOuter {
        ackId: String,
        message: PullMessage,
    }

    #[derive(Debug, Deserialize)]
    #[allow(non_snake_case)]
    struct PullMessage {
        data: String,
        messageId: String,
        publishTime: String,
    }

    impl PullMessage {
        fn decode_data(&self) -> TestMessage {
            let data = base64::decode(&self.data).expect("Invalid base64 data");
            let data = String::from_utf8_lossy(&data);
            serde_json::from_str(&data).expect("Invalid message structure")
        }
    }

    #[derive(Debug, Deserialize, Serialize)]
    struct TestMessage {
        timestamp: String,
        message: String,
    }
}
