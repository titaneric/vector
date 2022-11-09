mod common;
mod config;
mod encoder;
mod health;
mod request_builder;
mod retry;
mod service;
mod sink;

#[cfg(test)]
mod tests;

#[cfg(test)]
#[cfg(feature = "es-integration-tests")]
mod integration_tests;

use std::convert::TryFrom;

pub use common::*;
pub use config::*;
pub use encoder::ElasticsearchEncoder;
use http::{uri::InvalidUri, Request};
use snafu::Snafu;
use vector_common::sensitive_string::SensitiveString;
use vector_config::configurable_component;

use crate::aws::AwsAuthentication;
use crate::{
    event::{EventRef, LogEvent},
    internal_events::TemplateRenderingError,
    template::{Template, TemplateParseError},
};

/// Authentication strategies.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields, rename_all = "snake_case", tag = "strategy")]
pub enum ElasticsearchAuth {
    /// HTTP Basic Authentication.
    Basic {
        /// Basic authentication username.
        user: String,

        /// Basic authentication password.
        password: SensitiveString,
    },

    /// Amazon OpenSearch Service-specific authentication.
    Aws(#[configurable(derived)] AwsAuthentication),
}

/// Indexing mode.
#[configurable_component]
#[derive(Clone, Debug, Eq, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub enum ElasticsearchMode {
    /// Ingests documents in bulk, via the bulk API `index` action.
    #[serde(alias = "normal")]
    Bulk,

    /// Ingests documents in bulk, via the bulk API `create` action.
    ///
    /// Elasticsearch Data Streams only support the `create` action.
    DataStream,
}

impl Default for ElasticsearchMode {
    fn default() -> Self {
        Self::Bulk
    }
}

/// Bulk API actions.
#[configurable_component]
#[derive(Clone, Copy, Debug, Derivative, Eq, Hash, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub enum BulkAction {
    /// The `index` action.
    Index,

    /// The `create` action.
    Create,
}

#[allow(clippy::trivially_copy_pass_by_ref)]
impl BulkAction {
    pub const fn as_str(&self) -> &'static str {
        match self {
            BulkAction::Index => "index",
            BulkAction::Create => "create",
        }
    }

    pub const fn as_json_pointer(&self) -> &'static str {
        match self {
            BulkAction::Index => "/index",
            BulkAction::Create => "/create",
        }
    }
}

impl TryFrom<&str> for BulkAction {
    type Error = String;

    fn try_from(input: &str) -> Result<Self, Self::Error> {
        match input {
            "index" => Ok(BulkAction::Index),
            "create" => Ok(BulkAction::Create),
            _ => Err(format!("Invalid bulk action: {}", input)),
        }
    }
}

impl_generate_config_from_default!(ElasticsearchConfig);

#[derive(Debug, Clone)]
pub enum ElasticsearchCommonMode {
    Bulk {
        index: Template,
        action: Option<Template>,
    },
    DataStream(DataStreamConfig),
}

impl ElasticsearchCommonMode {
    fn index(&self, log: &LogEvent) -> Option<String> {
        match self {
            Self::Bulk { index, .. } => index
                .render_string(log)
                .map_err(|error| {
                    emit!(TemplateRenderingError {
                        error,
                        field: Some("index"),
                        drop_event: true,
                    });
                })
                .ok(),
            Self::DataStream(ds) => ds.index(log),
        }
    }

    fn bulk_action<'a>(&self, event: impl Into<EventRef<'a>>) -> Option<BulkAction> {
        match self {
            ElasticsearchCommonMode::Bulk {
                action: bulk_action,
                ..
            } => match bulk_action {
                Some(template) => template
                    .render_string(event)
                    .map_err(|error| {
                        emit!(TemplateRenderingError {
                            error,
                            field: Some("bulk_action"),
                            drop_event: true,
                        });
                    })
                    .ok()
                    .and_then(|value| BulkAction::try_from(value.as_str()).ok()),
                None => Some(BulkAction::Index),
            },
            // avoid the interpolation
            ElasticsearchCommonMode::DataStream(_) => Some(BulkAction::Create),
        }
    }

    const fn as_data_stream_config(&self) -> Option<&DataStreamConfig> {
        match self {
            Self::DataStream(value) => Some(value),
            _ => None,
        }
    }
}

/// Configuration for api version.
#[configurable_component]
#[derive(Clone, Debug, Eq, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub enum ElasticsearchApiVersion {
    /// Auto-detect the api version. Will fail if endpoint isn't reachable.
    Auto,
    /// Use the Elasticsearch 6.x API.
    V6,
    /// Use the Elasticsearch 7.x API.
    V7,
    /// Use the Elasticsearch 8.x API.
    V8,
}

impl Default for ElasticsearchApiVersion {
    fn default() -> Self {
        Self::Auto
    }
}

#[derive(Debug, Snafu)]
#[snafu(visibility(pub))]
pub enum ParseError {
    #[snafu(display("Invalid host {:?}: {:?}", host, source))]
    InvalidHost { host: String, source: InvalidUri },
    #[snafu(display("Host {:?} must include hostname", host))]
    HostMustIncludeHostname { host: String },
    #[snafu(display("Index template parse error: {}", source))]
    IndexTemplate { source: TemplateParseError },
    #[snafu(display("Batch action template parse error: {}", source))]
    BatchActionTemplate { source: TemplateParseError },
    #[snafu(display("aws.region required when AWS authentication is in use"))]
    RegionRequired,
    #[snafu(display("Endpoints option must be specified"))]
    EndpointRequired,
    #[snafu(display(
        "`endpoint` and `endpoints` options are mutually exclusive. Please use `endpoints` option."
    ))]
    EndpointsExclusive,
}
