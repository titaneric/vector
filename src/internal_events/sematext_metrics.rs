use metrics::counter;
use vector_core::internal_event::InternalEvent;

use crate::{emit, event::metric::Metric};
use vector_common::internal_event::{
    error_stage, error_type, ComponentEventsDropped, UNINTENTIONAL,
};

#[derive(Debug)]
pub struct SematextMetricsInvalidMetricError<'a> {
    pub metric: &'a Metric,
}

impl<'a> InternalEvent for SematextMetricsInvalidMetricError<'a> {
    fn emit(self) {
        let reason = "Invalid metric received.";
        error!(
            message = reason,
            error_code = "invalid_metric",
            error_type =  error_type::ENCODER_FAILED,
            stage = error_stage::PROCESSING,
            value = ?self.metric.value(),
            kind = ?self.metric.kind(),
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total", 1,
            "error_code" => "invalid_metric",
            "error_type" => error_type::ENCODER_FAILED,
            "stage" => error_stage::PROCESSING,
        );

        emit!(ComponentEventsDropped::<UNINTENTIONAL> { count: 1, reason });
    }
}

#[derive(Debug)]
pub struct SematextMetricsEncodeEventError<E> {
    pub error: E,
}

impl<E: std::fmt::Display> InternalEvent for SematextMetricsEncodeEventError<E> {
    fn emit(self) {
        let reason = "Failed to encode event.";
        error!(
            message = reason,
            error = %self.error,
            error_type = error_type::ENCODER_FAILED,
            stage = error_stage::PROCESSING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total", 1,
            "error_type" => error_type::ENCODER_FAILED,
            "stage" => error_stage::PROCESSING,
        );
        // deprecated
        counter!("encode_errors_total", 1);

        emit!(ComponentEventsDropped::<UNINTENTIONAL> { count: 1, reason });
    }
}
