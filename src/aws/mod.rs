pub mod auth;
pub mod region;

use std::sync::Arc;
use std::time::Duration;

pub use auth::AwsAuthentication;
use aws_config::meta::region::ProvideRegion;
use aws_smithy_async::rt::sleep::{AsyncSleep, Sleep};
use aws_smithy_client::erase::DynConnector;
use aws_smithy_client::SdkError;
use aws_smithy_http::endpoint::Endpoint;
use aws_smithy_types::retry::RetryConfig;
use aws_types::credentials::SharedCredentialsProvider;
use aws_types::region::Region;
use once_cell::sync::OnceCell;
use regex::RegexSet;
pub use region::RegionOrEndpoint;

use crate::config::ProxyConfig;
use crate::http::{build_proxy_connector, build_tls_connector};
use crate::tls::{MaybeTlsSettings, TlsConfig};

static RETRIABLE_CODES: OnceCell<RegexSet> = OnceCell::new();

pub fn is_retriable_error<T>(error: &SdkError<T>) -> bool {
    match error {
        SdkError::TimeoutError(_) | SdkError::DispatchFailure(_) => true,
        SdkError::ConstructionFailure(_) => false,
        SdkError::ResponseError { err: _, raw } | SdkError::ServiceError { err: _, raw } => {
            // This header is a direct indication that we should retry the request. Eventually it'd
            // be nice to actually schedule the retry after the given delay, but for now we just
            // check that it contains a positive value.
            let retry_header = raw.http().headers().get("x-amz-retry-after").is_some();

            // Certain 400-level responses will contain an error code indicating that the request
            // should be retried. Since we don't retry 400-level responses by default, we'll look
            // for these specifically before falling back to more general heuristics. Because AWS
            // services use a mix of XML and JSON response bodies and the AWS SDK doesn't give us
            // a parsed representation, we resort to a simple string match.
            //
            // S3: RequestTimeout
            // SQS: RequestExpired, ThrottlingException
            // ECS: RequestExpired, ThrottlingException
            // Kinesis: RequestExpired, ThrottlingException
            // Cloudwatch: RequestExpired, ThrottlingException
            //
            // Now just look for those when it's a client_error
            let re = RETRIABLE_CODES.get_or_init(|| {
                RegexSet::new(&["RequestTimeout", "RequestExpired", "ThrottlingException"])
                    .expect("invalid regex")
            });

            let status = raw.http().status();
            let response_body = String::from_utf8_lossy(raw.http().body().bytes().unwrap_or(&[]));

            retry_header
                || status.is_server_error()
                || status == http::StatusCode::TOO_MANY_REQUESTS
                || (status.is_client_error() && re.is_match(response_body.as_ref()))
        }
    }
}

pub trait ClientBuilder {
    type ConfigBuilder;
    type Client;

    fn create_config_builder(
        credentials_provider: SharedCredentialsProvider,
    ) -> Self::ConfigBuilder;

    fn with_endpoint_resolver(
        builder: Self::ConfigBuilder,
        endpoint: Endpoint,
    ) -> Self::ConfigBuilder;

    fn with_region(builder: Self::ConfigBuilder, region: Region) -> Self::ConfigBuilder;

    fn with_sleep_impl(
        builder: Self::ConfigBuilder,
        sleep_impl: Arc<dyn AsyncSleep>,
    ) -> Self::ConfigBuilder;

    fn with_retry_config(
        builder: Self::ConfigBuilder,
        retry_config: RetryConfig,
    ) -> Self::ConfigBuilder;

    fn client_from_conf_conn(builder: Self::ConfigBuilder, connector: DynConnector)
        -> Self::Client;
}

pub async fn create_client<T: ClientBuilder>(
    auth: &AwsAuthentication,
    region: Option<Region>,
    endpoint: Option<Endpoint>,
    proxy: &ProxyConfig,
    tls_options: &Option<TlsConfig>,
) -> crate::Result<T::Client> {
    // The default credentials chains will look for a region if not given but we'd like to
    // error up front if later SDK calls will fail due to lack of region configuration
    let region = match region {
        Some(region) => Ok(region),
        None => aws_config::default_provider::region::default_provider()
            .region()
            .await
            .ok_or("Could not determine region from Vector configuration or default providers"),
    }?;

    let mut config_builder =
        T::create_config_builder(auth.credentials_provider(region.clone()).await?);

    if let Some(endpoint_override) = endpoint {
        config_builder = T::with_endpoint_resolver(config_builder, endpoint_override);
    }

    config_builder = T::with_region(config_builder, region);

    let tls_settings = MaybeTlsSettings::tls_client(tls_options)?;

    let connector = if proxy.enabled {
        let proxy = build_proxy_connector(tls_settings, proxy)?;
        let hyper_client = aws_smithy_client::hyper_ext::Adapter::builder().build(proxy);
        aws_smithy_client::erase::DynConnector::new(hyper_client)
    } else {
        let tls_connector = build_tls_connector(tls_settings)?;
        let hyper_client = aws_smithy_client::hyper_ext::Adapter::builder().build(tls_connector);
        aws_smithy_client::erase::DynConnector::new(hyper_client)
    };

    config_builder = T::with_sleep_impl(config_builder, Arc::new(TokioSleep));
    // we disable retries because we have our own retry layer wired together with ARC to backoff
    config_builder = T::with_retry_config(config_builder, RetryConfig::disabled());

    Ok(T::client_from_conf_conn(config_builder, connector))
}

#[derive(Debug)]
pub struct TokioSleep;

impl AsyncSleep for TokioSleep {
    fn sleep(&self, duration: Duration) -> Sleep {
        Sleep::new(tokio::time::sleep(duration))
    }
}
