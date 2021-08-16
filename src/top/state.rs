use std::collections::btree_map::BTreeMap;
use tokio::sync::mpsc;

type IdentifiedMetric = (String, i64);

#[derive(Debug)]
pub enum EventType {
    EventsInTotals(Vec<IdentifiedMetric>),
    /// Interval in ms + identified metric
    EventsInThroughputs(i64, Vec<IdentifiedMetric>),
    EventsOutTotals(Vec<IdentifiedMetric>),
    /// Interval in ms + identified metric
    EventsOutThroughputs(i64, Vec<IdentifiedMetric>),
    ProcessedBytesTotals(Vec<IdentifiedMetric>),
    /// Interval + identified metric
    ProcessedBytesThroughputs(i64, Vec<IdentifiedMetric>),
    ComponentAdded(ComponentRow),
    ComponentRemoved(String),
}

pub type State = BTreeMap<String, ComponentRow>;
pub type EventTx = mpsc::Sender<EventType>;
pub type EventRx = mpsc::Receiver<EventType>;
pub type StateRx = mpsc::Receiver<State>;

#[derive(Debug, Clone)]
pub struct ComponentRow {
    pub id: String,
    pub kind: String,
    pub component_type: String,
    pub processed_bytes_total: i64,
    pub processed_bytes_throughput_sec: i64,
    pub events_in_total: i64,
    pub events_in_throughput_sec: i64,
    pub events_out_total: i64,
    pub events_out_throughput_sec: i64,
    pub errors: i64,
}

/// Takes the receiver `EventRx` channel, and returns a `StateTx` state transmitter. This
/// represents the single destination for handling subscriptions and returning 'immutable' state
/// for re-rendering the dashboard. This approach uses channels vs. mutexes.
pub async fn updater(mut state: State, mut event_rx: EventRx) -> StateRx {
    let (tx, rx) = mpsc::channel(20);

    // Prime the receiver with the initial state
    let _ = tx.send(state.clone()).await;

    tokio::spawn(async move {
        loop {
            if let Some(event_type) = event_rx.recv().await {
                match event_type {
                    EventType::EventsInTotals(rows) => {
                        for (id, v) in rows {
                            if let Some(r) = state.get_mut(&id) {
                                r.events_in_total = v;
                            }
                        }
                    }
                    EventType::EventsInThroughputs(interval, rows) => {
                        for (id, v) in rows {
                            if let Some(r) = state.get_mut(&id) {
                                r.events_in_throughput_sec =
                                    (v as f64 * (1000.0 / interval as f64)) as i64;
                            }
                        }
                    }
                    EventType::EventsOutTotals(rows) => {
                        for (id, v) in rows {
                            if let Some(r) = state.get_mut(&id) {
                                r.events_out_total = v;
                            }
                        }
                    }
                    EventType::EventsOutThroughputs(interval, rows) => {
                        for (id, v) in rows {
                            if let Some(r) = state.get_mut(&id) {
                                r.events_out_throughput_sec =
                                    (v as f64 * (1000.0 / interval as f64)) as i64;
                            }
                        }
                    }
                    EventType::ProcessedBytesTotals(rows) => {
                        for (id, v) in rows {
                            if let Some(r) = state.get_mut(&id) {
                                r.processed_bytes_total = v;
                            }
                        }
                    }
                    EventType::ProcessedBytesThroughputs(interval, rows) => {
                        for (id, v) in rows {
                            if let Some(r) = state.get_mut(&id) {
                                r.processed_bytes_throughput_sec =
                                    (v as f64 * (1000.0 / interval as f64)) as i64;
                            }
                        }
                    }
                    EventType::ComponentAdded(c) => {
                        let _ = state.insert(c.id.clone(), c);
                    }
                    EventType::ComponentRemoved(id) => {
                        let _ = state.remove(&id);
                    }
                }

                // Send updated map to listeners
                let _ = tx.send(state.clone()).await;
            }
        }
    });

    rx
}
