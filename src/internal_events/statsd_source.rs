// ## skip check-events ##

use bytes::Bytes;
use metrics::counter;
use vector_core::internal_event::InternalEvent;

#[derive(Debug)]
pub struct StatsdInvalidRecord<'a> {
    pub error: &'a crate::sources::statsd::parser::ParseError,
    pub bytes: Bytes,
}

impl<'a> InternalEvent for StatsdInvalidRecord<'a> {
    fn emit_logs(&self) {
        error!(message = "Invalid packet from statsd, discarding.", error = ?self.error, bytes = %String::from_utf8_lossy(&self.bytes));
    }

    fn emit_metrics(&self) {
        counter!("invalid_record_total", 1,);
        counter!("invalid_record_bytes_total", self.bytes.len() as u64);
    }
}

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
}

impl<T: std::fmt::Debug + std::fmt::Display> InternalEvent for StatsdSocketError<T> {
    fn emit_logs(&self) {
        let message = match self.r#type {
            StatsdSocketErrorType::Bind => "Failed to bind to UDP listener socket.",
            StatsdSocketErrorType::Read => "Failed to read UDP datagram.",
        };
        error!(message, error = ?self.error);
    }

    fn emit_metrics(&self) {
        counter!("connection_errors_total", 1);
    }
}
