use crate::{
    emit,
    internal_events::{ComponentEventsDropped, UNINTENTIONAL},
};
use metrics::counter;
use vector_core::internal_event::InternalEvent;

use vector_common::internal_event::{error_stage, error_type};

#[derive(Debug)]
pub struct LargeEventDroppedError {
    pub(crate) length: usize,
    pub max_length: usize,
}

impl InternalEvent for LargeEventDroppedError {
    fn emit(self) {
        error!(
            message = "Event larger than batch max_bytes.",
            batch_max_bytes = %self.max_length,
            length = %self.length,
            error_type = error_type::CONDITION_FAILED,
            stage = error_stage::SENDING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total", 1,
            "error_code" => "oversized",
            "error_type" => error_type::CONDITION_FAILED,
            "stage" => error_stage::SENDING,
        );
        emit!(ComponentEventsDropped::<UNINTENTIONAL> {
            count: 1,
            reason: "Event larger than batch max_bytes."
        });
        // deprecated
        counter!(
            "events_discarded_total", 1,
            "reason" => "oversized",
        );
    }
}
