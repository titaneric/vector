use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct EventsReceived {
    pub count: usize,
    pub byte_size: usize,
}

impl InternalEvent for EventsReceived {
    fn emit_logs(&self) {
        trace!(message = "Events received.", count = %self.count, byte_size = %self.byte_size);
    }

    fn emit_metrics(&self) {
        counter!("received_events_total", self.count as u64);
        counter!("events_in_total", self.count as u64);
        counter!("received_event_bytes_total", self.byte_size as u64);
    }
}

#[derive(Debug)]
pub struct EventsSent {
    pub count: usize,
    pub byte_size: usize,
}

impl InternalEvent for EventsSent {
    fn emit_logs(&self) {
        trace!(message = "Events sent.", count = %self.count, byte_size = %self.byte_size);
    }

    fn emit_metrics(&self) {
        if self.count > 0 {
            counter!("events_out_total", self.count as u64);
            counter!("sent_events_total", self.count as u64);
            counter!("sent_event_bytes_total", self.byte_size as u64);
        }
    }
}
