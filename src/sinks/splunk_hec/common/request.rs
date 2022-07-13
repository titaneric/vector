use std::sync::Arc;

use bytes::Bytes;
use vector_core::{
    event::{EventFinalizers, Finalizable},
    ByteSizeOf,
};

use crate::sinks::util::ElementCount;

#[derive(Clone, Debug)]
pub struct HecRequest {
    pub body: Bytes,
    pub events_count: usize,
    pub events_byte_size: usize,
    pub finalizers: EventFinalizers,
    pub passthrough_token: Option<Arc<str>>,
    pub index: Option<String>,
    pub source: Option<String>,
    pub sourcetype: Option<String>,
    pub host: Option<String>,
}

impl ByteSizeOf for HecRequest {
    fn allocated_bytes(&self) -> usize {
        self.body.allocated_bytes() + self.finalizers.allocated_bytes()
    }
}

impl ElementCount for HecRequest {
    fn element_count(&self) -> usize {
        self.events_count
    }
}

impl Finalizable for HecRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        std::mem::take(&mut self.finalizers)
    }
}
