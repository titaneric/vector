use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct HTTPEventsReceived {
    pub events_count: usize,
    pub byte_size: usize,
}

impl InternalEvent for HTTPEventsReceived {
    fn emit_logs(&self) {
        trace!(
            message = "Sending events.",
            events_count = %self.events_count,
            byte_size = %self.byte_size,
        );
    }

    fn emit_metrics(&self) {
        counter!("processed_events_total", self.events_count as u64);
        counter!("processed_bytes_total", self.byte_size as u64);
    }
}

#[derive(Debug)]
pub struct HTTPBadRequest<'a> {
    pub error_code: u16,
    pub error_message: &'a str,
}

impl<'a> InternalEvent for HTTPBadRequest<'a> {
    fn emit_logs(&self) {
        warn!(
            message = "Received bad request.",
            code = ?self.error_code,
            error_message = ?self.error_message,
            rate_limit_secs = 10,
        );
    }

    fn emit_metrics(&self) {
        counter!("http_bad_requests_total", 1);
    }
}
