// ## skip check-events ##

use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct StdinEventsReceived {
    pub byte_size: usize,
    pub count: usize,
}

impl InternalEvent for StdinEventsReceived {
    fn emit_logs(&self) {
        trace!(
            message = "Events received.",
            count = self.count,
            byte_size = self.byte_size,
        );
    }

    fn emit_metrics(&self) {
        counter!("component_received_events_total", self.count as u64);
        counter!(
            "component_received_event_bytes_total",
            self.byte_size as u64
        );
        // deprecated
        counter!("events_in_total", self.count as u64);
        counter!("processed_bytes_total", self.byte_size as u64);
    }
}
