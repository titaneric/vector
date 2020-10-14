use super::InternalEvent;
use metrics::counter;

#[derive(Debug)]
pub struct UnixSocketConnectionEstablished<'a> {
    pub path: &'a std::path::Path,
}

impl InternalEvent for UnixSocketConnectionEstablished<'_> {
    fn emit_logs(&self) {
        debug!(message = "Connected.", path = ?self.path);
    }

    fn emit_metrics(&self) {
        counter!("unix_socket_connections_established", 1);
    }
}

#[derive(Debug)]
pub struct UnixSocketConnectionFailure<'a, E> {
    pub error: E,
    pub path: &'a std::path::Path,
}

impl<E: std::error::Error> InternalEvent for UnixSocketConnectionFailure<'_, E> {
    fn emit_logs(&self) {
        error!(
            message = "Unix socket connection failure.",
            error = %self.error,
            path = ?self.path,
        );
    }

    fn emit_metrics(&self) {
        counter!("unix_socket_connection_failures", 1);
    }
}

#[derive(Debug)]
pub struct UnixSocketSendFailed<'a, E> {
    pub error: E,
    pub path: &'a std::path::Path,
}

impl<E: std::error::Error> InternalEvent for UnixSocketSendFailed<'_, E> {
    fn emit_logs(&self) {
        error!(
            message = "Unix socket send failed.",
            error = %self.error,
            path = ?self.path,
        );
    }

    fn emit_metrics(&self) {
        counter!("unix_socket_send_errors", 1,
            "component_kind" => "sink",
            "component_type" => "socket",
            "mode" => "unix");
    }
}

#[derive(Debug)]
pub struct UnixSocketFlushFailed<'a, E> {
    pub error: E,
    pub path: &'a std::path::Path,
}

impl<E: std::error::Error> InternalEvent for UnixSocketFlushFailed<'_, E> {
    fn emit_logs(&self) {
        error!(
            message = "Flush failed.",
            error = %self.error,
            path = ?self.path,
        );
    }

    fn emit_metrics(&self) {
        counter!("unix_socket_flush_errors", 1,
            "component_kind" => "sink",
            "component_type" => "socket",
            "mode" => "unix");
    }
}

#[derive(Debug)]
pub struct UnixSocketEventSent {
    pub byte_size: usize,
}

impl InternalEvent for UnixSocketEventSent {
    fn emit_metrics(&self) {
        counter!("events_processed", 1, "mode" => "unix");
        counter!("bytes_processed", self.byte_size as u64, "mode" => "unix");
    }
}

#[derive(Debug)]
pub struct UnixSocketReceiveFailed<'a, E> {
    pub error: E,
    pub path: &'a std::path::Path,
}

impl<E: std::error::Error> InternalEvent for UnixSocketReceiveFailed<'_, E> {
    fn emit_logs(&self) {
        error!(
            message = "Error receiving data.",
            error = %self.error,
            path = ?self.path,
        );
    }

    fn emit_metrics(&self) {
        counter!("unix_socket_errors", 1,
            "component_kind" => "source",
            "component_type" => "socket",
            "mode" => "unix");
    }
}

#[derive(Debug)]
pub struct UnixSocketError<'a, E> {
    pub error: E,
    pub path: &'a std::path::Path,
}

impl<E: From<std::io::Error> + std::fmt::Debug + std::fmt::Display> InternalEvent
    for UnixSocketError<'_, E>
{
    fn emit_logs(&self) {
        debug!(
            message = "Unix socket error.",
            error = %self.error,
            path = ?self.path,
        );
    }

    fn emit_metrics(&self) {
        counter!("unix_socket_errors", 1);
    }
}
