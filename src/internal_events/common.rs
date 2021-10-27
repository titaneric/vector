use metrics::counter;
use vector_core::internal_event::InternalEvent;

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
        counter!("component_received_events_total", self.count as u64);
        counter!("events_in_total", self.count as u64);
        counter!(
            "component_received_event_bytes_total",
            self.byte_size as u64
        );
    }
}

#[derive(Debug)]
pub struct HttpClientBytesReceived<'a> {
    pub byte_size: usize,
    pub protocol: &'a str,
    pub endpoint: &'a str,
}

impl InternalEvent for HttpClientBytesReceived<'_> {
    fn emit_logs(&self) {
        trace!(
            message = "Bytes received.",
            byte_size = %self.byte_size,
            protocol = %self.protocol,
            endpoint = %self.endpoint,
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_received_bytes_total", self.byte_size as u64,
            "protocol" => self.protocol.to_owned(),
            "endpoint" => self.endpoint.to_owned(),
        );
    }
}

#[derive(Debug)]
pub struct BytesSent<'a> {
    pub byte_size: usize,
    pub protocol: &'a str,
}

impl<'a> InternalEvent for BytesSent<'a> {
    fn emit_logs(&self) {
        trace!(message = "Bytes sent.", byte_size = %self.byte_size, protocol = %self.protocol);
    }

    fn emit_metrics(&self) {
        counter!("component_sent_bytes_total", self.byte_size as u64,
                 "protocol" => self.protocol.to_string());
    }
}

#[derive(Debug)]
pub struct EndpointBytesSent<'a> {
    pub byte_size: usize,
    pub protocol: &'a str,
    pub endpoint: &'a str,
}

impl<'a> InternalEvent for EndpointBytesSent<'a> {
    fn emit_logs(&self) {
        trace!(
            message = "Bytes sent.",
            byte_size = %self.byte_size,
            protocol = %self.protocol,
            endpoint = %self.endpoint
        );
    }

    fn emit_metrics(&self) {
        counter!(
            "component_sent_bytes_total", self.byte_size as u64,
            "protocol" => self.protocol.to_string(),
            "endpoint" => self.endpoint.to_string()
        );
    }
}

#[cfg(feature = "rusoto")]
pub struct AwsBytesSent {
    pub byte_size: usize,
    pub region: rusoto_core::Region,
}

#[cfg(feature = "rusoto")]
impl InternalEvent for AwsBytesSent {
    fn emit_logs(&self) {
        trace!(message = "Bytes sent.", byte_size = %self.byte_size, region = ?self.region);
    }

    fn emit_metrics(&self) {
        counter!(
            "component_sent_bytes_total", self.byte_size as u64,
            "protocol" => "https",
            "region" => self.region.name().to_owned(),
        );
    }
}
