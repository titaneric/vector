use super::healthcheck::healthcheck;
use super::sink::LokiSink;
use crate::config::{DataType, GenerateConfig, SinkConfig, SinkContext};
use crate::http::{Auth, HttpClient, MaybeAuth};
use crate::sinks::util::encoding::EncodingConfig;
use crate::sinks::util::{BatchConfig, TowerRequestConfig, UriSerde};
use crate::sinks::VectorSink;
use crate::template::Template;
use crate::tls::{TlsOptions, TlsSettings};
use futures::future::FutureExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LokiConfig {
    pub endpoint: UriSerde,
    pub encoding: EncodingConfig<Encoding>,

    pub tenant_id: Option<Template>,
    pub labels: HashMap<Template, Template>,

    #[serde(default = "crate::serde::default_false")]
    pub remove_label_fields: bool,
    #[serde(default = "crate::serde::default_true")]
    pub remove_timestamp: bool,
    #[serde(default)]
    pub out_of_order_action: OutOfOrderAction,

    pub auth: Option<Auth>,

    #[serde(default)]
    pub request: TowerRequestConfig,

    #[serde(default)]
    pub batch: BatchConfig,

    pub tls: Option<TlsOptions>,
}

#[derive(Clone, Debug, Derivative, Deserialize, Serialize)]
#[derivative(Default)]
#[serde(rename_all = "snake_case")]
pub enum OutOfOrderAction {
    #[derivative(Default)]
    Drop,
    RewriteTimestamp,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    Json,
    Text,
    Logfmt,
}

impl GenerateConfig for LokiConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"endpoint = "http://localhost:3100"
            encoding = "json"
            labels = {}"#,
        )
        .unwrap()
    }
}

impl LokiConfig {
    pub(super) fn build_client(&self, cx: SinkContext) -> crate::Result<HttpClient> {
        let tls = TlsSettings::from_options(&self.tls)?;
        let client = HttpClient::new(tls, cx.proxy())?;
        Ok(client)
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "loki")]
impl SinkConfig for LokiConfig {
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(VectorSink, crate::sinks::Healthcheck)> {
        if self.labels.is_empty() {
            return Err("`labels` must include at least one label.".into());
        }

        for label in self.labels.keys() {
            if !valid_label_name(label) {
                return Err(format!("Invalid label name {:?}", label.get_ref()).into());
            }
        }

        let client = self.build_client(cx.clone())?;

        let config = LokiConfig {
            auth: self.auth.choose_one(&self.endpoint.auth)?,
            ..self.clone()
        };

        let sink = LokiSink::new(config.clone(), client.clone(), cx)?;

        let healthcheck = healthcheck(config, client).boxed();

        Ok((VectorSink::Stream(Box::new(sink)), healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "loki"
    }
}

pub fn valid_label_name(label: &Template) -> bool {
    label.is_dynamic() || {
        // Loki follows prometheus on this https://prometheus.io/docs/concepts/data_model/#metric-names-and-labels
        // Although that isn't explicitly said anywhere besides what's in the code.
        // The closest mention is in section about Parser Expression https://grafana.com/docs/loki/latest/logql/
        //
        // [a-zA-Z_][a-zA-Z0-9_]*
        let label_trim = label.get_ref().trim();
        let mut label_chars = label_trim.chars();
        if let Some(ch) = label_chars.next() {
            (ch.is_ascii_alphabetic() || ch == '_')
                && label_chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::valid_label_name;
    use std::convert::TryInto;

    #[test]
    fn valid_label_names() {
        assert!(valid_label_name(&"name".try_into().unwrap()));
        assert!(valid_label_name(&" name ".try_into().unwrap()));
        assert!(valid_label_name(&"bee_bop".try_into().unwrap()));
        assert!(valid_label_name(&"a09b".try_into().unwrap()));

        assert!(!valid_label_name(&"0ab".try_into().unwrap()));
        assert!(!valid_label_name(&"*".try_into().unwrap()));
        assert!(!valid_label_name(&"".try_into().unwrap()));
        assert!(!valid_label_name(&" ".try_into().unwrap()));

        assert!(valid_label_name(&"{{field}}".try_into().unwrap()));
    }
}
