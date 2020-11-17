#[cfg(feature = "api")]
#[macro_use]
extern crate matches;

mod support;

#[cfg(all(feature = "api", feature = "vector-api-client"))]
mod tests {
    use crate::support::{sink, source_with_event_counter};
    use chrono::Utc;
    use futures::StreamExt;
    use std::collections::HashMap;
    use std::{
        net::SocketAddr,
        sync::Once,
        time::{Duration, Instant},
    };
    use tokio::{select, sync::oneshot};
    use url::Url;
    use vector::{
        self,
        api::{self, Server},
        config::{self, Config},
        internal_events::{emit, GeneratorEventProcessed, Heartbeat},
        test_util::{next_addr, retry_until},
    };
    use vector_api_client::{
        connect_subscription_client,
        gql::{
            ComponentsSubscriptionExt, HealthQueryExt, HealthSubscriptionExt, MetaQueryExt,
            MetricsSubscriptionExt,
        },
        Client, SubscriptionClient,
    };

    static METRICS_INIT: Once = Once::new();

    // Initialize the metrics system. Idempotent.
    fn init_metrics() -> oneshot::Sender<()> {
        METRICS_INIT.call_once(|| {
            vector::trace::init(true, true, "info");
            let _ = vector::metrics::init();
        });

        let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();
        tokio::spawn(async move {
            let since = Instant::now();
            let mut timer = tokio::time::interval(Duration::from_secs(1));

            loop {
                select! {
                    _ = &mut shutdown_rx => break,
                    _ = timer.tick() => {
                        emit(Heartbeat { since });
                    }
                }
            }
        });

        shutdown_tx
    }

    // Provides a config that enables the API server, assigned to a random port. Implicitly
    // tests that the config shape matches expectations
    fn api_enabled_config() -> Config {
        let mut config = Config::builder();
        config.add_source("in1", source_with_event_counter().1);
        config.add_sink("out1", &["in1"], sink(10).1);
        config.api.enabled = true;
        config.api.bind = Some(next_addr());

        config.build().unwrap()
    }

    async fn from_str_config(conf: &str) -> vector::topology::RunningTopology {
        let mut c = config::load_from_str(conf).unwrap();
        c.api.bind = Some(next_addr());

        let diff = config::ConfigDiff::initial(&c);
        let pieces = vector::topology::build_or_log_errors(&c, &diff)
            .await
            .unwrap();

        let result = vector::topology::start_validated(c, diff, pieces, false).await;
        let (topology, _graceful_crash) = result.unwrap();

        topology
    }

    // Starts and returns the server
    fn start_server() -> Server {
        let config = api_enabled_config();
        api::Server::start(&config)
    }

    fn make_client(addr: SocketAddr) -> Client {
        let url = Url::parse(&*format!("http://{}/graphql", addr)).unwrap();

        Client::new(url)
    }

    // Returns the result of a URL test against the API. Wraps the test in retry_until
    // to guard against the race condition of the TCP listener not being ready
    async fn url_test(config: Config, url: &'static str) -> reqwest::Response {
        let addr = config.api.bind.unwrap();
        let url = format!("http://{}:{}/{}", addr.ip(), addr.port(), url);

        let _server = api::Server::start(&config);

        // Build the request
        let client = reqwest::Client::new();

        retry_until(
            || client.get(&url).send(),
            Duration::from_millis(100),
            Duration::from_secs(10),
        )
        .await
    }

    // Creates and returns a new subscription client. Connection is re-attempted until
    // the specified timeout
    async fn new_subscription_client(addr: SocketAddr) -> SubscriptionClient {
        let url = Url::parse(&*format!("ws://{}/graphql", addr)).unwrap();

        retry_until(
            || connect_subscription_client(url.clone()),
            Duration::from_millis(50),
            Duration::from_secs(10),
        )
        .await
    }

    // Emits fake generate events every 10ms until the returned shutdown falls out of scope
    fn emit_fake_generator_events() -> oneshot::Sender<()> {
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();
        tokio::spawn(async move {
            let mut timer = tokio::time::interval(Duration::from_millis(10));

            loop {
                select! {
                    _ = &mut shutdown_rx => break,
                    _ = timer.tick() => {
                        emit(GeneratorEventProcessed);
                    }
                }
            }
        });

        shutdown_tx
    }

    async fn new_heartbeat_subscription(
        client: &SubscriptionClient,
        num_results: usize,
        interval: i64,
    ) {
        let subscription = client.heartbeat_subscription(interval);

        tokio::pin! {
            let heartbeats = subscription.stream().take(num_results);
        }

        // Should get 3x timestamps that are at least `interval` apart. The first one
        // will be almost immediate, so move it by `interval` to account for the diff
        let now = Utc::now() - chrono::Duration::milliseconds(interval);

        for mul in 1..=num_results {
            let diff = heartbeats
                .next()
                .await
                .unwrap()
                .unwrap()
                .data
                .unwrap()
                .heartbeat
                .utc
                - now;

            assert!(diff.num_milliseconds() >= mul as i64 * interval);
        }

        // Stream should have stopped after `num_results`
        assert_matches!(heartbeats.next().await, None);
    }

    async fn new_uptime_subscription(client: &SubscriptionClient) {
        let subscription = client.uptime_subscription();

        tokio::pin! {
            let uptime = subscription.stream().skip(1);
        }

        // Uptime should be above zero
        assert!(
            uptime
                .take(1)
                .next()
                .await
                .unwrap()
                .unwrap()
                .data
                .unwrap()
                .uptime
                .seconds
                > 0.00
        )
    }

    async fn new_events_processed_total_subscription(
        client: &SubscriptionClient,
        num_results: usize,
        interval: i64,
    ) {
        // Emit events for the duration of the test
        let _shutdown = emit_fake_generator_events();

        let subscription = client.events_processed_total_subscription(interval);

        tokio::pin! {
            let events_processed_total = subscription.stream().take(num_results);
        }

        let mut last_result = 0.0;

        for _ in 0..num_results {
            let ep = events_processed_total
                .next()
                .await
                .unwrap()
                .unwrap()
                .data
                .unwrap()
                .events_processed_total
                .events_processed_total;

            assert!(ep > last_result);
            last_result = ep
        }
    }

    #[tokio::test]
    /// Tests the /health endpoint returns a 200 responses (non-GraphQL)
    async fn api_health() {
        let res = url_test(api_enabled_config(), "health")
            .await
            .text()
            .await
            .unwrap();

        assert!(res.contains("ok"));
    }

    #[tokio::test]
    /// Tests that the API playground is enabled when playground = true (implicit)
    async fn api_playground_enabled() {
        let mut config = api_enabled_config();
        config.api.playground = true;

        let res = url_test(config, "playground").await.status();

        assert!(res.is_success());
    }

    #[tokio::test]
    /// Tests that the /playground URL is inaccessible if it's been explicitly disabled
    async fn api_playground_disabled() {
        let mut config = api_enabled_config();
        config.api.playground = false;

        let res = url_test(config, "playground").await.status();

        assert!(res.is_client_error());
    }

    #[tokio::test]
    /// Tests the health query
    async fn api_graphql_health() {
        let server = start_server();
        let client = make_client(server.addr());

        let res = client.health_query().await.unwrap();

        assert!(res.data.unwrap().health);
        assert_eq!(res.errors, None);
    }

    #[tokio::test]
    /// tests that version_string meta matches the current Vector version
    async fn api_graphql_meta_version_string() {
        let server = start_server();
        let client = make_client(server.addr());

        let res = client.meta_version_string().await.unwrap();

        assert_eq!(res.data.unwrap().meta.version_string, vector::get_version());
    }

    #[tokio::test]
    /// Tests that the heartbeat subscription returns a UTC payload every 1/2 second
    async fn api_graphql_heartbeat() {
        let server = start_server();
        let client = new_subscription_client(server.addr()).await;

        new_heartbeat_subscription(&client, 3, 500).await;
    }

    #[tokio::test]
    /// Tests for Vector instance uptime in seconds
    async fn api_graphql_uptime_metrics() {
        let server = start_server();
        let client = new_subscription_client(server.addr()).await;

        let _metrics = init_metrics();

        new_uptime_subscription(&client).await;
    }

    #[tokio::test]
    /// Tests for events processed metrics, using fake generator events
    async fn api_graphql_event_processed_total_metrics() {
        let server = start_server();
        let client = new_subscription_client(server.addr()).await;

        let _metrics = init_metrics();

        new_events_processed_total_subscription(&client, 3, 100).await;
    }

    #[tokio::test]
    /// Tests whether 2 disparate subscriptions can run against a single client
    async fn api_graphql_combined_heartbeat_uptime() {
        let server = start_server();
        let client = new_subscription_client(server.addr()).await;

        let _metrics = init_metrics();

        futures::join! {
            new_uptime_subscription(&client),
            new_heartbeat_subscription(&client, 3, 500),
        };
    }

    #[tokio::test]
    #[allow(clippy::float_cmp)]
    #[ignore]
    /// Tests componentEventsProcessedTotal returns increasing metrics, ordered by
    /// source -> transform -> sink
    async fn api_graphql_component_events_processed_total() {
        init_metrics();

        let topology = from_str_config(
            r#"
            [api]
              enabled = true

            [sources.events_processed_total_source]
              type = "generator"
              lines = ["Random line", "And another"]
              batch_interval = 0.1

            [sinks.events_processed_total_sink]
              # General
              type = "blackhole"
              inputs = ["events_processed_total_source"]
              print_amount = 100000
        "#,
        )
        .await;

        let server = api::Server::start(topology.config());
        let client = new_subscription_client(server.addr()).await;
        let subscription = client.component_events_processed_total_subscription(500);

        tokio::pin! {
            let component_events_processed_total = subscription.stream();
        }

        // Results should be sorted by source -> sink, so we'll need to assert that
        // order. The events generated should be the same in both cases
        let mut map = HashMap::new();

        for r in 0..=1 {
            map.insert(
                r,
                component_events_processed_total
                    .next()
                    .await
                    .unwrap()
                    .unwrap()
                    .data
                    .unwrap()
                    .component_events_processed_total,
            );
        }

        assert_eq!(map[&0].name, "events_processed_total_source");
        assert_eq!(map[&1].name, "events_processed_total_sink");

        assert_eq!(
            map[&0].metric.events_processed_total,
            map[&1].metric.events_processed_total
        );
    }

    #[tokio::test]
    #[allow(clippy::float_cmp)]
    #[ignore]
    /// Tests componentEventsProcessedTotalBatch returns increasing metrics, ordered by
    /// source -> transform -> sink
    async fn api_graphql_component_events_processed_total_batch() {
        init_metrics();

        let topology = from_str_config(
            r#"
            [api]
              enabled = true

            [sources.events_processed_total_batch_source]
              type = "generator"
              lines = ["Random line", "And another"]
              batch_interval = 0.1

            [sinks.events_processed_total_batch_sink]
              # General
              type = "blackhole"
              inputs = ["events_processed_total_batch_source"]
              print_amount = 100000
        "#,
        )
        .await;

        let server = api::Server::start(topology.config());
        let client = new_subscription_client(server.addr()).await;
        let subscription = client.component_events_processed_total_batch_subscription(500);

        tokio::pin! {
            let component_events_processed_total_batch = subscription.stream();
        }

        let data = component_events_processed_total_batch
            .next()
            .await
            .unwrap()
            .unwrap()
            .data
            .unwrap()
            .component_events_processed_total_batch;

        assert_eq!(data[0].name, "events_processed_total_batch_source");
        assert_eq!(data[1].name, "events_processed_total_batch_sink");

        assert_eq!(
            data[0].metric.events_processed_total,
            data[1].metric.events_processed_total
        );
    }

    #[tokio::test]
    #[allow(clippy::float_cmp)]
    #[ignore]
    /// Tests componentBytesProcessedTotal returns increasing metrics, ordered by
    /// source -> transform -> sink
    async fn api_graphql_component_bytes_processed_total() {
        init_metrics();

        let topology = from_str_config(
            r#"
            [api]
              enabled = true

            [sources.bytes_processed_total_source]
              type = "generator"
              lines = ["Random line", "And another"]
              batch_interval = 0.1

            [sinks.bytes_processed_total_sink]
              # General
              type = "blackhole"
              inputs = ["bytes_processed_total_source"]
              print_amount = 100000
        "#,
        )
        .await;

        let server = api::Server::start(topology.config());
        let client = new_subscription_client(server.addr()).await;
        let subscription = client.component_bytes_processed_total_subscription(500);

        tokio::pin! {
            let component_bytes_processed_total = subscription.stream();
        }

        // Results should be sorted by source -> sink, so we'll need to assert that
        // order. The events generated should be the same in both cases
        let mut map = HashMap::new();

        for r in 0..=1 {
            map.insert(
                r,
                component_bytes_processed_total
                    .next()
                    .await
                    .unwrap()
                    .unwrap()
                    .data
                    .unwrap()
                    .component_bytes_processed_total,
            );
        }

        // Should only show byte totals on sinks
        assert_eq!(map[&0].name, "bytes_processed_total_sink");
        assert_eq!(map[&1].name, "bytes_processed_total_sink");

        assert!(map[&0].metric.bytes_processed_total > 0.00);
        assert!(map[&1].metric.bytes_processed_total > map[&0].metric.bytes_processed_total)
    }

    #[tokio::test]
    #[allow(clippy::float_cmp)]
    #[ignore]
    /// Tests componentBytesProcessedTotalBatch returns increasing metrics, ordered by
    /// source -> transform -> sink
    async fn api_graphql_component_bytes_processed_total_batch() {
        init_metrics();

        let topology = from_str_config(
            r#"
            [api]
              enabled = true

            [sources.bytes_processed_total_batch_source]
              type = "generator"
              lines = ["Random line", "And another"]
              batch_interval = 0.1

            [sinks.bytes_processed_total_batch_sink]
              # General
              type = "blackhole"
              inputs = ["bytes_processed_total_batch_source"]
              print_amount = 100000
        "#,
        )
        .await;

        let server = api::Server::start(topology.config());
        let client = new_subscription_client(server.addr()).await;
        let subscription = client.component_bytes_processed_total_batch_subscription(500);

        tokio::pin! {
            let component_bytes_processed_total_batch = subscription.stream();
        }

        let data = component_bytes_processed_total_batch
            .next()
            .await
            .unwrap()
            .unwrap()
            .data
            .unwrap()
            .component_bytes_processed_total_batch;

        // Bytes are currently only relevant on sinks
        assert_eq!(data[0].name, "bytes_processed_total_batch_sink");
        assert!(data[0].metric.bytes_processed_total > 0.00);
    }

    #[tokio::test]
    #[ignore]
    /// Tests componentAdded receives an added component
    async fn api_graphql_component_added_subscription() {
        init_metrics();

        // Initial topology
        let mut topology = from_str_config(
            r#"
            [api]
              enabled = true

            [sources.component_added_source_1]
              type = "generator"
              lines = ["Random line", "And another"]
              batch_interval = 0.1

            [sinks.component_added_sink]
              # General
              type = "blackhole"
              inputs = ["component_added_source_1"]
              print_amount = 100000
        "#,
        )
        .await;

        let server = api::Server::start(topology.config());
        let client = new_subscription_client(server.addr()).await;

        // Spawn a handler for listening to changes
        let handle = tokio::spawn(async move {
            let subscription = client.component_added();

            tokio::pin! {
                let component_added = subscription.stream();
            }

            assert_eq!(
                "component_added_source_2",
                component_added
                    .next()
                    .await
                    .unwrap()
                    .unwrap()
                    .data
                    .unwrap()
                    .component_added
                    .name,
            );
        });

        // After a short delay, update the config to include `gen2`
        tokio::time::delay_for(tokio::time::Duration::from_millis(200)).await;

        let c = config::load_from_str(
            r#"
            [api]
              enabled = true

            [sources.component_added_source_1]
              type = "generator"
              lines = ["Random line", "And another"]
              batch_interval = 0.1

            [sources.component_added_source_2]
              type = "generator"
              lines = ["3rd line", "4th line"]
              batch_interval = 0.1

            [sinks.component_added_sink]
              # General
              type = "blackhole"
              inputs = ["component_added_source_1", "component_added_source_2"]
              print_amount = 100000
        "#,
        )
        .unwrap();

        topology.reload_config_and_respawn(c, false).await.unwrap();
        server.update_config(topology.config());

        // Await the join handle
        handle.await.unwrap();
    }

    #[tokio::test]
    #[ignore]
    /// Tests componentRemoves detects when a component has been removed
    async fn api_graphql_component_removed_subscription() {
        init_metrics();

        // Initial topology
        let mut topology = from_str_config(
            r#"
            [api]
              enabled = true

            [sources.component_removed_source_1]
              type = "generator"
              lines = ["Random line", "And another"]
              batch_interval = 0.1

            [sources.component_removed_source_2]
              type = "generator"
              lines = ["3rd line", "4th line"]
              batch_interval = 0.1

            [sinks.component_removed_sink]
              # General
              type = "blackhole"
              inputs = ["component_removed_source_1", "component_removed_source_2"]
              print_amount = 100000
        "#,
        )
        .await;

        let server = api::Server::start(topology.config());
        let client = new_subscription_client(server.addr()).await;

        // Spawn a handler for listening to changes
        let handle = tokio::spawn(async move {
            let subscription = client.component_removed();

            tokio::pin! {
                let component_removed = subscription.stream();
            }

            assert_eq!(
                "component_removed_source_2",
                component_removed
                    .next()
                    .await
                    .unwrap()
                    .unwrap()
                    .data
                    .unwrap()
                    .component_removed
                    .name,
            );
        });

        // After a short delay, update the config to remove `gen2`
        tokio::time::delay_for(tokio::time::Duration::from_millis(200)).await;

        let c = config::load_from_str(
            r#"
            [api]
              enabled = true

            [sources.component_removed_source_1]
              type = "generator"
              lines = ["Random line", "And another"]
              batch_interval = 0.1

            [sinks.component_removed_sink]
              # General
              type = "blackhole"
              inputs = ["component_removed_source_1"]
              print_amount = 100000
        "#,
        )
        .unwrap();

        topology.reload_config_and_respawn(c, false).await.unwrap();
        server.update_config(topology.config());

        // Await the join handle
        handle.await.unwrap();
    }
}
