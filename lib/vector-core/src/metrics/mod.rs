mod handle;
mod label_filter;
mod recorder;
mod registry;

use crate::event::{Event, Metric};
pub use crate::metrics::handle::{Counter, Handle};
use crate::metrics::label_filter::VectorLabelFilter;
use crate::metrics::recorder::VectorRecorder;
use crate::metrics::registry::VectorRegistry;
use metrics::{Key, SharedString};
use metrics_tracing_context::TracingContextLayer;
use metrics_util::layers::Layer;
use metrics_util::{CompositeKey, MetricKind};
use once_cell::sync::OnceCell;

static CONTROLLER: OnceCell<Controller> = OnceCell::new();
// Cardinality counter parameters, expose the internal metrics registry
// cardinality. Useful for the end users to help understand the characteristics
// of their environment and how vectors acts in it.
const CARDINALITY_KEY_NAME: &str = "internal_metrics_cardinality_total";
static CARDINALITY_KEY_DATA_NAME: [SharedString; 1] =
    [SharedString::const_str(&CARDINALITY_KEY_NAME)];
static CARDINALITY_KEY: CompositeKey = CompositeKey::new(
    MetricKind::Counter,
    Key::from_static_name(&CARDINALITY_KEY_DATA_NAME),
);

/// Controller allows capturing metric snapshots.
pub struct Controller {
    registry: VectorRegistry<CompositeKey, Handle>,
}

fn metrics_enabled() -> bool {
    !matches!(std::env::var("DISABLE_INTERNAL_METRICS_CORE"), Ok(x) if x == "true")
}

fn tracing_context_layer_enabled() -> bool {
    !matches!(std::env::var("DISABLE_INTERNAL_METRICS_TRACING_INTEGRATION"), Ok(x) if x == "true")
}

/// Initialize the metrics sub-system
///
/// # Errors
///
/// This function will error if it is called multiple times.
pub fn init() -> crate::Result<()> {
    // An escape hatch to allow disabing internal metrics core. May be used for
    // performance reasons. This is a hidden and undocumented functionality.
    if !metrics_enabled() {
        metrics::set_boxed_recorder(Box::new(metrics::NoopRecorder))
            .map_err(|_| "recorder already initialized")?;
        info!(message = "Internal metrics core is disabled.");
        return Ok(());
    }

    ////
    //// Prepare the registry
    ////
    let registry = VectorRegistry::default();

    ////
    //// Prepare the controller
    ////

    // The `Controller` is a safe spot in memory for us to stash a clone of the
    // registry -- where metrics are actually kept -- so that our sub-systems
    // interested in these metrics can grab copies. See `capture_metrics` and
    // its callers for an example.
    let controller = Controller {
        registry: registry.clone(),
    };
    CONTROLLER
        .set(controller)
        .map_err(|_| "controller already initialized")?;

    ////
    //// Initialize the recorder.
    ////

    // The recorder is the interface between metrics-rs and our registry. In our
    // case it doesn't _do_ much other than shepherd into the registry and
    // update the cardinality counter, see above, as needed.
    let recorder = VectorRecorder::new(registry);
    let recorder: Box<dyn metrics::Recorder> = if tracing_context_layer_enabled() {
        // Apply a layer to capture tracing span fields as labels.
        Box::new(TracingContextLayer::new(VectorLabelFilter).layer(recorder))
    } else {
        Box::new(recorder)
    };

    // This where we combine metrics-rs and our registry. We box it to avoid
    // having to fiddle with statics ourselves.
    metrics::set_boxed_recorder(recorder).map_err(|_| "recorder already initialized")?;

    Ok(())
}

/// Clear all metrics from the registry.
pub fn reset(controller: &Controller) {
    controller.registry.map.clear()
}

/// Get a handle to the globally registered controller, if it's initialized.
///
/// # Errors
///
/// This function will fail if the metrics subsystem has not been correctly
/// initialized.
pub fn get_controller() -> crate::Result<&'static Controller> {
    CONTROLLER
        .get()
        .ok_or_else(|| "metrics system not initialized".into())
}

/// Take a snapshot of all gathered metrics and expose them as metric
/// [`Event`]s.
pub fn capture_metrics(controller: &Controller) -> impl Iterator<Item = Event> {
    let mut events = controller
        .registry
        .map
        .iter()
        .map(|kv| Metric::from_metric_kv(kv.key().key(), kv.value()).into())
        .collect::<Vec<Event>>();

    // Add alias `events_processed_total` for `events_out_total`.
    for i in 0..events.len() {
        let metric = events[i].as_metric();
        if metric.name() == "events_out_total" {
            let alias = metric.clone().with_name("processed_events_total");
            events.push(alias.into());
        }
    }

    let handle = Handle::Counter(Counter::with_count(events.len() as u64 + 1));
    events.push(Metric::from_metric_kv(CARDINALITY_KEY.key(), &handle).into());

    events.into_iter()
}

#[macro_export]
/// This macro is used to emit metrics as a `counter` while simultaneously
/// converting from absolute values to incremental values.
///
/// Values that do not arrive in strictly monotonically increasing order are
/// ignored and will not be emitted.
macro_rules! update_counter {
    ($label:literal, $value:expr) => {{
        use ::std::sync::atomic::{AtomicU64, Ordering};

        static PREVIOUS_VALUE: AtomicU64 = AtomicU64::new(0);

        let new_value = $value;
        let mut previous_value = PREVIOUS_VALUE.load(Ordering::Relaxed);

        loop {
            // Either a new greater value has been emitted before this thread updated the counter
            // or values were provided that are not in strictly monotonically increasing order.
            // Ignore.
            if new_value <= previous_value {
                break;
            }

            match PREVIOUS_VALUE.compare_exchange_weak(
                previous_value,
                new_value,
                Ordering::SeqCst,
                Ordering::Relaxed,
            ) {
                // Another thread has written a new value before us. Re-enter loop.
                Err(value) => previous_value = value,
                // Calculate delta to last emitted value and emit it.
                Ok(_) => {
                    let delta = new_value - previous_value;
                    // Albeit very unlikely, note that this sequence of deltas might be emitted in
                    // a different order than they were calculated.
                    counter!($label, delta);
                    break;
                }
            }
        }
    }};
}
