use metrics::counter;
use vector_core::internal_event::InternalEvent;

use vector_common::internal_event::{error_stage, error_type};

#[derive(Debug)]
enum StatsdSocketErrorType {
    Bind,
    Read,
}

#[derive(Debug)]
pub struct StatsdSocketError<T> {
    r#type: StatsdSocketErrorType,
    pub error: T,
}

impl<T> StatsdSocketError<T> {
    const fn new(r#type: StatsdSocketErrorType, error: T) -> Self {
        Self { r#type, error }
    }

    pub const fn bind(error: T) -> Self {
        Self::new(StatsdSocketErrorType::Bind, error)
    }

    #[allow(clippy::missing_const_for_fn)] // const cannot run destructor
    pub fn read(error: T) -> Self {
        Self::new(StatsdSocketErrorType::Read, error)
    }

    const fn error_code(&self) -> &'static str {
        match self.r#type {
            StatsdSocketErrorType::Bind => "failed_udp_binding",
            StatsdSocketErrorType::Read => "failed_udp_datagram",
        }
    }
}

impl<T: std::fmt::Debug + std::fmt::Display> InternalEvent for StatsdSocketError<T> {
    fn emit(self) {
        let message = match self.r#type {
            StatsdSocketErrorType::Bind => {
                format!("Failed to bind to UDP listener socket: {:?}", self.error)
            }
            StatsdSocketErrorType::Read => format!("Failed to read UDP datagram: {:?}", self.error),
        };
        let error_code = self.error_code();
        error!(
            message = %message,
            error_code = %error_code,
            error_type = error_type::CONNECTION_FAILED,
            stage = error_stage::RECEIVING,
            internal_log_rate_limit = true,
        );
        counter!(
            "component_errors_total", 1,
            "error_code" => error_code,
            "error_type" => error_type::CONNECTION_FAILED,
            "stage" => error_stage::RECEIVING,
        );
        // deprecated
        counter!("connection_errors_total", 1);
    }
}
