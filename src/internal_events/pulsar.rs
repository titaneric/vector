// ## skip check-events ##

use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub(crate) struct PulsarEncodeEventFailed<'a> {
    pub error: &'a str,
}

impl<'a> InternalEvent for PulsarEncodeEventFailed<'a> {
    fn emit_logs(&self) {
        error!(
            message = "Event encode failed; dropping event.",
            error = %self.error,
            internal_log_rate_secs = 30,
        );
    }

    fn emit_metrics(&self) {
        counter!("encode_errors_total", 1);
    }
}
