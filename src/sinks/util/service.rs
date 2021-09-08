use crate::buffers::Acker;
use crate::sinks::util::adaptive_concurrency::{
    AdaptiveConcurrencyLimit, AdaptiveConcurrencyLimitLayer, AdaptiveConcurrencySettings,
};
use crate::sinks::util::retries::{FixedRetryPolicy, RetryLogic};
pub use crate::sinks::util::service::concurrency::{concurrency_is_none, Concurrency};
pub use crate::sinks::util::service::map::Map;
use crate::sinks::util::service::map::MapLayer;
use crate::sinks::util::sink::{Response, ServiceLogic};
use crate::sinks::util::{Batch, BatchSink, Partition, PartitionBatchSink};
use serde::{Deserialize, Serialize};
use std::{hash::Hash, sync::Arc, time::Duration};
use tower::{
    layer::{util::Stack, Layer},
    limit::RateLimit,
    retry::Retry,
    timeout::Timeout,
    util::BoxService,
    Service, ServiceBuilder,
};

mod concurrency;
mod map;

pub type Svc<S, L> = RateLimit<Retry<FixedRetryPolicy<L>, AdaptiveConcurrencyLimit<Timeout<S>, L>>>;
pub type TowerBatchedSink<S, B, RL, SL> = BatchSink<Svc<S, RL>, B, SL>;
pub type TowerPartitionSink<S, B, RL, K, SL> = PartitionBatchSink<Svc<S, RL>, B, K, SL>;

pub trait ServiceBuilderExt<L> {
    fn map<R1, R2, F>(self, f: F) -> ServiceBuilder<Stack<MapLayer<R1, R2>, L>>
    where
        F: Fn(R1) -> R2 + Send + Sync + 'static;

    fn settings<RL, Request>(
        self,
        settings: TowerRequestSettings,
        retry_logic: RL,
    ) -> ServiceBuilder<Stack<TowerRequestLayer<RL, Request>, L>>;
}

impl<L> ServiceBuilderExt<L> for ServiceBuilder<L> {
    fn map<R1, R2, F>(self, f: F) -> ServiceBuilder<Stack<MapLayer<R1, R2>, L>>
    where
        F: Fn(R1) -> R2 + Send + Sync + 'static,
    {
        self.layer(MapLayer::new(Arc::new(f)))
    }

    fn settings<RL, Request>(
        self,
        settings: TowerRequestSettings,
        retry_logic: RL,
    ) -> ServiceBuilder<Stack<TowerRequestLayer<RL, Request>, L>> {
        self.layer(TowerRequestLayer {
            settings,
            retry_logic,
            _pd: std::marker::PhantomData,
        })
    }
}

/// Tower Request based configuration
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct TowerRequestConfig {
    #[serde(default)]
    #[serde(skip_serializing_if = "concurrency_is_none")]
    pub concurrency: Concurrency, // 1024
    /// The same as concurrency but with old deprecated name.
    /// Alias couldn't be used because of <https://github.com/serde-rs/serde/issues/1504>
    #[serde(default)]
    #[serde(skip_serializing_if = "concurrency_is_none")]
    pub in_flight_limit: Concurrency, // 1024
    pub timeout_secs: Option<u64>,             // 1 minute
    pub rate_limit_duration_secs: Option<u64>, // 1 second
    pub rate_limit_num: Option<u64>,           // i64::MAX
    pub retry_attempts: Option<usize>,         // isize::MAX
    pub retry_max_duration_secs: Option<u64>,
    pub retry_initial_backoff_secs: Option<u64>, // 1
    #[serde(default)]
    pub adaptive_concurrency: AdaptiveConcurrencySettings,
}

pub const CONCURRENCY_DEFAULT: Concurrency = Concurrency::None;
pub const RATE_LIMIT_DURATION_SECONDS_DEFAULT: u64 = 1; // one second
pub const RATE_LIMIT_NUM_DEFAULT: u64 = i64::max_value() as u64; // i64 avoids TOML deserialize issue
pub const RETRY_ATTEMPTS_DEFAULT: usize = isize::max_value() as usize; // isize avoids TOML deserialize issue
pub const RETRY_MAX_DURATION_SECONDS_DEFAULT: u64 = 3_600; // one hour
pub const RETRY_INITIAL_BACKOFF_SECONDS_DEFAULT: u64 = 1; // one second
pub const TIMEOUT_SECONDS_DEFAULT: u64 = 60; // one minute

impl Default for TowerRequestConfig {
    fn default() -> Self {
        Self {
            concurrency: CONCURRENCY_DEFAULT,
            in_flight_limit: CONCURRENCY_DEFAULT,
            timeout_secs: Some(TIMEOUT_SECONDS_DEFAULT),
            rate_limit_duration_secs: Some(RATE_LIMIT_DURATION_SECONDS_DEFAULT),
            rate_limit_num: Some(RATE_LIMIT_NUM_DEFAULT),
            retry_attempts: Some(RETRY_ATTEMPTS_DEFAULT),
            retry_max_duration_secs: Some(RETRY_MAX_DURATION_SECONDS_DEFAULT),
            retry_initial_backoff_secs: Some(RETRY_INITIAL_BACKOFF_SECONDS_DEFAULT),
            adaptive_concurrency: AdaptiveConcurrencySettings::default(),
        }
    }
}

impl TowerRequestConfig {
    pub const fn const_new(concurrency: Concurrency, in_flight_limit: Concurrency) -> Self {
        Self {
            concurrency,
            in_flight_limit,
            timeout_secs: Some(TIMEOUT_SECONDS_DEFAULT),
            rate_limit_duration_secs: Some(RATE_LIMIT_DURATION_SECONDS_DEFAULT),
            rate_limit_num: Some(RATE_LIMIT_NUM_DEFAULT),
            retry_attempts: Some(RETRY_ATTEMPTS_DEFAULT),
            retry_max_duration_secs: Some(RETRY_MAX_DURATION_SECONDS_DEFAULT),
            retry_initial_backoff_secs: Some(RETRY_INITIAL_BACKOFF_SECONDS_DEFAULT),
            adaptive_concurrency: AdaptiveConcurrencySettings::const_default(),
        }
    }

    pub const fn rate_limit_num(mut self, rate_limit_num: u64) -> Self {
        self.rate_limit_num = Some(rate_limit_num);
        self
    }

    pub fn unwrap_with(&self, defaults: &Self) -> TowerRequestSettings {
        TowerRequestSettings {
            concurrency: self.concurrency().parse_concurrency(defaults.concurrency()),
            timeout: Duration::from_secs(
                self.timeout_secs
                    .or(defaults.timeout_secs)
                    .unwrap_or(TIMEOUT_SECONDS_DEFAULT),
            ),
            rate_limit_duration: Duration::from_secs(
                self.rate_limit_duration_secs
                    .or(defaults.rate_limit_duration_secs)
                    .unwrap_or(RATE_LIMIT_DURATION_SECONDS_DEFAULT),
            ),
            rate_limit_num: self
                .rate_limit_num
                .or(defaults.rate_limit_num)
                .unwrap_or(RATE_LIMIT_NUM_DEFAULT),
            retry_attempts: self
                .retry_attempts
                .or(defaults.retry_attempts)
                .unwrap_or(RETRY_ATTEMPTS_DEFAULT),
            retry_max_duration_secs: Duration::from_secs(
                self.retry_max_duration_secs
                    .or(defaults.retry_max_duration_secs)
                    .unwrap_or(RETRY_MAX_DURATION_SECONDS_DEFAULT),
            ),
            retry_initial_backoff_secs: Duration::from_secs(
                self.retry_initial_backoff_secs
                    .or(defaults.retry_initial_backoff_secs)
                    .unwrap_or(RETRY_INITIAL_BACKOFF_SECONDS_DEFAULT),
            ),
            adaptive_concurrency: self.adaptive_concurrency,
        }
    }

    pub fn concurrency(&self) -> &Concurrency {
        match (
            concurrency_is_none(&self.concurrency),
            concurrency_is_none(&self.in_flight_limit),
        ) {
            (_, true) => &self.concurrency,
            (true, false) => &self.in_flight_limit,
            (false, false) => {
                warn!("Option `in_flight_limit` has been renamed to `concurrency`. Ignoring `in_flight_limit` and using `concurrency` option.");
                &self.concurrency
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct TowerRequestSettings {
    pub concurrency: Option<usize>,
    pub timeout: Duration,
    pub rate_limit_duration: Duration,
    pub rate_limit_num: u64,
    pub retry_attempts: usize,
    pub retry_max_duration_secs: Duration,
    pub retry_initial_backoff_secs: Duration,
    pub adaptive_concurrency: AdaptiveConcurrencySettings,
}

impl TowerRequestSettings {
    pub fn retry_policy<L: RetryLogic>(&self, logic: L) -> FixedRetryPolicy<L> {
        FixedRetryPolicy::new(
            self.retry_attempts,
            self.retry_initial_backoff_secs,
            self.retry_max_duration_secs,
            logic,
        )
    }

    pub fn partition_sink<B, RL, S, K, SL>(
        &self,
        retry_logic: RL,
        service: S,
        batch: B,
        batch_timeout: Duration,
        acker: Acker,
        service_logic: SL,
    ) -> TowerPartitionSink<S, B, RL, K, SL>
    where
        RL: RetryLogic<Response = S::Response>,
        S: Service<B::Output> + Clone + Send + 'static,
        S::Error: Into<crate::Error> + Send + Sync + 'static,
        S::Response: Send + Response,
        S::Future: Send + 'static,
        B: Batch,
        B::Input: Partition<K>,
        B::Output: Send + Clone + 'static,
        K: Hash + Eq + Clone + Send + 'static,
        SL: ServiceLogic<Response = S::Response> + Send + 'static,
    {
        PartitionBatchSink::new_with_logic(
            self.service(retry_logic, service),
            batch,
            batch_timeout,
            acker,
            service_logic,
        )
    }

    pub fn batch_sink<B, RL, S, SL>(
        &self,
        retry_logic: RL,
        service: S,
        batch: B,
        batch_timeout: Duration,
        acker: Acker,
        service_logic: SL,
    ) -> TowerBatchedSink<S, B, RL, SL>
    where
        RL: RetryLogic<Response = S::Response>,
        S: Service<B::Output> + Clone + Send + 'static,
        S::Error: Into<crate::Error> + Send + Sync + 'static,
        S::Response: Send + Response,
        S::Future: Send + 'static,
        B: Batch,
        B::Output: Send + Clone + 'static,
        SL: ServiceLogic<Response = S::Response> + Send + 'static,
    {
        BatchSink::new_with_logic(
            self.service(retry_logic, service),
            batch,
            batch_timeout,
            acker,
            service_logic,
        )
    }

    pub fn service<RL, S, Request>(&self, retry_logic: RL, service: S) -> Svc<S, RL>
    where
        RL: RetryLogic<Response = S::Response>,
        S: Service<Request> + Clone + Send + 'static,
        S::Error: Into<crate::Error> + Send + Sync + 'static,
        S::Response: Send + Response,
        S::Future: Send + 'static,
        Request: Send + Clone + 'static,
    {
        let policy = self.retry_policy(retry_logic.clone());
        ServiceBuilder::new()
            .rate_limit(self.rate_limit_num, self.rate_limit_duration)
            .retry(policy)
            .layer(AdaptiveConcurrencyLimitLayer::new(
                self.concurrency,
                self.adaptive_concurrency,
                retry_logic,
            ))
            .timeout(self.timeout)
            .service(service)
    }
}

#[derive(Debug, Clone)]
pub struct TowerRequestLayer<L, Request> {
    settings: TowerRequestSettings,
    retry_logic: L,
    _pd: std::marker::PhantomData<Request>,
}

impl<S, RL, Request> Layer<S> for TowerRequestLayer<RL, Request>
where
    S: Service<Request> + Send + Clone + 'static,
    S::Response: Response + Send + 'static,
    S::Error: Into<crate::Error> + Send + Sync + 'static,
    S::Future: Send + 'static,
    RL: RetryLogic<Response = S::Response> + Send + 'static,
    Request: Clone + Send + 'static,
{
    type Service = BoxService<Request, S::Response, crate::Error>;

    fn layer(&self, inner: S) -> Self::Service {
        let policy = self.settings.retry_policy(self.retry_logic.clone());

        let l = ServiceBuilder::new()
            .concurrency_limit(self.settings.concurrency.unwrap_or(5))
            .rate_limit(
                self.settings.rate_limit_num,
                self.settings.rate_limit_duration,
            )
            .retry(policy)
            .timeout(self.settings.timeout)
            .service(inner);

        BoxService::new(l)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        buffers::Acker,
        sinks::util::{
            retries::{RetryAction, RetryLogic},
            sink::StdServiceLogic,
            BatchSettings, EncodedEvent, PartitionBuffer, PartitionInnerBuffer, VecBuffer,
        },
    };
    use futures::{future, stream, FutureExt, SinkExt, StreamExt};
    use std::sync::{
        atomic::{AtomicBool, Ordering::AcqRel},
        Arc, Mutex,
    };
    use tokio::time::Duration;

    const TIMEOUT: Duration = Duration::from_secs(10);

    #[test]
    fn concurrency_param_works() {
        let cfg = TowerRequestConfig::default();
        let toml = toml::to_string(&cfg).unwrap();
        toml::from_str::<TowerRequestConfig>(&toml).expect("Default config failed");

        let cfg = toml::from_str::<TowerRequestConfig>("").expect("Empty config failed");
        assert_eq!(cfg.concurrency, Concurrency::None);

        let cfg = toml::from_str::<TowerRequestConfig>("concurrency = 10")
            .expect("Fixed concurrency failed");
        assert_eq!(cfg.concurrency, Concurrency::Fixed(10));

        let cfg = toml::from_str::<TowerRequestConfig>(r#"concurrency = "adaptive""#)
            .expect("Adaptive concurrency setting failed");
        assert_eq!(cfg.concurrency, Concurrency::Adaptive);

        toml::from_str::<TowerRequestConfig>(r#"concurrency = "broken""#)
            .expect_err("Invalid concurrency setting didn't fail");

        toml::from_str::<TowerRequestConfig>(r#"concurrency = 0"#)
            .expect_err("Invalid concurrency setting didn't fail on zero");

        toml::from_str::<TowerRequestConfig>(r#"concurrency = -9"#)
            .expect_err("Invalid concurrency setting didn't fail on negative number");
    }

    #[test]
    fn backward_compatibility_with_in_flight_limit_param_works() {
        let cfg = toml::from_str::<TowerRequestConfig>("in_flight_limit = 10")
            .expect("Fixed concurrency failed for in_flight_limit param");
        assert_eq!(cfg.concurrency(), &Concurrency::Fixed(10));
    }

    #[tokio::test]
    async fn partition_sink_retry_concurrency() {
        let cfg = TowerRequestConfig {
            concurrency: Concurrency::Fixed(1),
            ..TowerRequestConfig::default()
        };
        let settings = cfg.unwrap_with(&TowerRequestConfig::default());

        let (acker, _) = Acker::new_for_testing();
        let sent_requests = Arc::new(Mutex::new(Vec::new()));

        let svc = {
            let sent_requests = Arc::clone(&sent_requests);
            let delay = Arc::new(AtomicBool::new(true));
            tower::service_fn(move |req: PartitionInnerBuffer<_, _>| {
                let (req, _) = req.into_parts();
                if delay.swap(false, AcqRel) {
                    // Error on first request
                    future::err::<(), _>(std::io::Error::new(std::io::ErrorKind::Other, "")).boxed()
                } else {
                    sent_requests.lock().unwrap().push(req);
                    future::ok::<_, std::io::Error>(()).boxed()
                }
            })
        };

        let batch = BatchSettings::default().bytes(9999).events(10);
        let mut sink = settings.partition_sink(
            RetryAlways,
            svc,
            PartitionBuffer::new(VecBuffer::new(batch.size)),
            TIMEOUT,
            acker,
            StdServiceLogic::default(),
        );
        sink.ordered();

        let input = (0..20).into_iter().map(|i| PartitionInnerBuffer::new(i, 0));
        sink.sink_map_err(drop)
            .send_all(&mut stream::iter(input).map(|item| Ok(EncodedEvent::new(item))))
            .await
            .unwrap();

        let output = sent_requests.lock().unwrap();
        assert_eq!(
            &*output,
            &vec![
                (0..10).into_iter().collect::<Vec<_>>(),
                (10..20).into_iter().collect::<Vec<_>>(),
            ]
        );
    }

    #[derive(Clone, Debug, Copy)]
    struct RetryAlways;

    impl RetryLogic for RetryAlways {
        type Error = std::io::Error;
        type Response = ();

        fn is_retriable_error(&self, _: &Self::Error) -> bool {
            true
        }

        fn should_retry_response(&self, _response: &Self::Response) -> RetryAction {
            // Treat the default as the request is successful
            RetryAction::Successful
        }
    }
}
