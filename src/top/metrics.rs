use super::state;
use std::sync::Arc;
use tokio_stream::StreamExt;
use vector_api_client::{
    gql::{ComponentsQueryExt, ComponentsSubscriptionExt, MetricsSubscriptionExt},
    Client, SubscriptionClient,
};

/// Components that have been added
async fn component_added(client: Arc<SubscriptionClient>, tx: state::EventTx) {
    let res = client.component_added();

    tokio::pin! {
        let stream = res.stream();
    };

    while let Some(Some(res)) = stream.next().await {
        if let Some(d) = res.data {
            let c = d.component_added;
            let _ = tx
                .send(state::EventType::ComponentAdded(state::ComponentRow {
                    name: c.name,
                    kind: c.on.to_string(),
                    component_type: c.component_type,
                    events_in_total: 0,
                    events_in_throughput_sec: 0,
                    events_out_total: 0,
                    events_out_throughput_sec: 0,
                    processed_bytes_total: 0,
                    processed_bytes_throughput_sec: 0,
                    errors: 0,
                }))
                .await;
        }
    }
}

/// Components that have been removed
async fn component_removed(client: Arc<SubscriptionClient>, tx: state::EventTx) {
    let res = client.component_removed();

    tokio::pin! {
        let stream = res.stream();
    };

    while let Some(Some(res)) = stream.next().await {
        if let Some(d) = res.data {
            let c = d.component_removed;
            let _ = tx.send(state::EventType::ComponentRemoved(c.name)).await;
        }
    }
}

async fn events_in_totals(client: Arc<SubscriptionClient>, tx: state::EventTx, interval: i64) {
    let res = client.component_events_in_totals_subscription(interval);

    tokio::pin! {
        let stream = res.stream();
    };

    while let Some(Some(res)) = stream.next().await {
        if let Some(d) = res.data {
            let c = d.component_events_in_totals;
            let _ = tx
                .send(state::EventType::EventsInTotals(
                    c.into_iter()
                        .map(|c| (c.name, c.metric.events_in_total as i64))
                        .collect(),
                ))
                .await;
        }
    }
}

async fn events_in_throughputs(client: Arc<SubscriptionClient>, tx: state::EventTx, interval: i64) {
    let res = client.component_events_in_throughputs_subscription(interval);

    tokio::pin! {
        let stream = res.stream();
    };

    while let Some(Some(res)) = stream.next().await {
        if let Some(d) = res.data {
            let c = d.component_events_in_throughputs;
            let _ = tx
                .send(state::EventType::EventsInThroughputs(
                    interval,
                    c.into_iter().map(|c| (c.name, c.throughput)).collect(),
                ))
                .await;
        }
    }
}

async fn events_out_totals(client: Arc<SubscriptionClient>, tx: state::EventTx, interval: i64) {
    let res = client.component_events_out_totals_subscription(interval);

    tokio::pin! {
        let stream = res.stream();
    };

    while let Some(Some(res)) = stream.next().await {
        if let Some(d) = res.data {
            let c = d.component_events_out_totals;
            let _ = tx
                .send(state::EventType::EventsOutTotals(
                    c.into_iter()
                        .map(|c| (c.name, c.metric.events_out_total as i64))
                        .collect(),
                ))
                .await;
        }
    }
}

async fn events_out_throughputs(
    client: Arc<SubscriptionClient>,
    tx: state::EventTx,
    interval: i64,
) {
    let res = client.component_events_out_throughputs_subscription(interval);

    tokio::pin! {
        let stream = res.stream();
    };

    while let Some(Some(res)) = stream.next().await {
        if let Some(d) = res.data {
            let c = d.component_events_out_throughputs;
            let _ = tx
                .send(state::EventType::EventsOutThroughputs(
                    interval,
                    c.into_iter().map(|c| (c.name, c.throughput)).collect(),
                ))
                .await;
        }
    }
}

async fn processed_bytes_totals(
    client: Arc<SubscriptionClient>,
    tx: state::EventTx,
    interval: i64,
) {
    let res = client.component_processed_bytes_totals_subscription(interval);

    tokio::pin! {
        let stream = res.stream();
    };

    while let Some(Some(res)) = stream.next().await {
        if let Some(d) = res.data {
            let c = d.component_processed_bytes_totals;
            let _ = tx
                .send(state::EventType::ProcessedBytesTotals(
                    c.into_iter()
                        .map(|c| (c.name, c.metric.processed_bytes_total as i64))
                        .collect(),
                ))
                .await;
        }
    }
}

async fn processed_bytes_throughputs(
    client: Arc<SubscriptionClient>,
    tx: state::EventTx,
    interval: i64,
) {
    let res = client.component_processed_bytes_throughputs_subscription(interval);

    tokio::pin! {
        let stream = res.stream();
    };

    while let Some(Some(res)) = stream.next().await {
        if let Some(d) = res.data {
            let c = d.component_processed_bytes_throughputs;
            let _ = tx
                .send(state::EventType::ProcessedBytesThroughputs(
                    interval,
                    c.into_iter().map(|c| (c.name, c.throughput)).collect(),
                ))
                .await;
        }
    }
}

/// Subscribe to each metrics channel through a separate client. This is a temporary workaround
/// until client multiplexing is fixed. In future, we should be able to use a single client
pub fn subscribe(client: SubscriptionClient, tx: state::EventTx, interval: i64) {
    let client = Arc::new(client);

    tokio::spawn(component_added(Arc::clone(&client), tx.clone()));
    tokio::spawn(component_removed(Arc::clone(&client), tx.clone()));
    tokio::spawn(events_in_totals(Arc::clone(&client), tx.clone(), interval));
    tokio::spawn(events_in_throughputs(
        Arc::clone(&client),
        tx.clone(),
        interval,
    ));
    tokio::spawn(events_out_totals(Arc::clone(&client), tx.clone(), interval));
    tokio::spawn(events_out_throughputs(
        Arc::clone(&client),
        tx.clone(),
        interval,
    ));
    tokio::spawn(processed_bytes_totals(
        Arc::clone(&client),
        tx.clone(),
        interval,
    ));
    tokio::spawn(processed_bytes_throughputs(
        Arc::clone(&client),
        tx,
        interval,
    ));
}

/// Retrieve the initial components/metrics for first paint. Further updating the metrics
/// will be handled by subscriptions.
pub async fn init_components(client: &Client) -> Result<state::State, ()> {
    // Execute a query to get the latest components, and aggregate metrics for each resource.
    // Since we don't know currently have a mechanism for scrolling/paging through results,
    // we're using an artificially high page size to capture all likely component configurations.
    let rows = client
        .components_query(i16::max_value() as i64)
        .await
        .map_err(|_| ())?
        .data
        .ok_or(())?
        .components
        .edges
        .into_iter()
        .flat_map(|d| {
            d.into_iter().filter_map(|edge| {
                let d = edge?.node;
                Some((
                    d.name.clone(),
                    state::ComponentRow {
                        name: d.name,
                        kind: d.on.to_string(),
                        component_type: d.component_type,
                        events_in_total: d.on.events_in_total(),
                        events_in_throughput_sec: 0,
                        events_out_total: d.on.events_out_total(),
                        events_out_throughput_sec: 0,
                        processed_bytes_total: d.on.processed_bytes_total(),
                        processed_bytes_throughput_sec: 0,

                        errors: 0,
                    },
                ))
            })
        })
        .collect::<state::State>();

    Ok(rows)
}
