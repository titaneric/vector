// ## skip check-events ##

use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug, Copy, Clone)]
pub struct VrlConditionExecutionError<'a> {
    pub error: &'a str,
}

impl<'a> InternalEvent for VrlConditionExecutionError<'a> {
    fn emit(self) {
        error!(
            message = "VRL condition execution failed.",
            error = %self.error,
            internal_log_rate_secs = 120
        );
        counter!("processing_errors_total", 1);
    }
}
