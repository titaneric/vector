use core_common::internal_event::emit;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::{sync::Arc, time::Duration};
use tokio::time::interval;
use tracing::{Instrument, Span};

use crate::internal_events::{
    BufferCreated, BufferEventsReceived, BufferEventsSent, EventsDropped,
};
use crate::WhenFull;

pub struct BufferUsageData {
    received_event_count: AtomicU64,
    received_byte_size: AtomicUsize,
    sent_event_count: AtomicU64,
    sent_byte_size: AtomicUsize,
    dropped_event_count: Option<AtomicU64>,
    max_size_bytes: Option<usize>,
    max_size_events: Option<usize>,
}

impl BufferUsageData {
    pub fn new(
        when_full: WhenFull,
        span: Span,
        max_size_bytes: Option<usize>,
        max_size_events: Option<usize>,
    ) -> Arc<Self> {
        let dropped_event_count = match when_full {
            WhenFull::Block => None,
            WhenFull::DropNewest => Some(AtomicU64::new(0)),
        };

        let buffer_usage_data = Arc::new(Self {
            received_event_count: AtomicU64::new(0),
            received_byte_size: AtomicUsize::new(0),
            sent_event_count: AtomicU64::new(0),
            sent_byte_size: AtomicUsize::new(0),
            dropped_event_count,
            max_size_bytes,
            max_size_events,
        });

        let usage_data = buffer_usage_data.clone();
        tokio::spawn(
            async move {
                let mut interval = interval(Duration::from_secs(2));
                loop {
                    interval.tick().await;

                    emit(&BufferCreated {
                        max_size_bytes: usage_data.max_size_bytes,
                        max_size_events: usage_data.max_size_events,
                    });

                    emit(&BufferEventsReceived {
                        count: usage_data.received_event_count.swap(0, Ordering::Relaxed),
                        byte_size: usage_data.received_byte_size.swap(0, Ordering::Relaxed),
                    });

                    emit(&BufferEventsSent {
                        count: usage_data.sent_event_count.swap(0, Ordering::Relaxed),
                        byte_size: usage_data.sent_byte_size.swap(0, Ordering::Relaxed),
                    });

                    if let Some(dropped_event_count) = &usage_data.dropped_event_count {
                        emit(&EventsDropped {
                            count: dropped_event_count.swap(0, Ordering::Relaxed),
                        });
                    }
                }
            }
            .instrument(span),
        );

        buffer_usage_data
    }

    pub fn increment_received_event_count_and_byte_size(&self, count: u64, byte_size: usize) {
        self.received_event_count
            .fetch_add(count, Ordering::Relaxed);
        self.received_byte_size
            .fetch_add(byte_size, Ordering::Relaxed);
    }

    pub fn increment_sent_event_count_and_byte_size(&self, count: u64, byte_size: usize) {
        self.sent_event_count.fetch_add(count, Ordering::Relaxed);
        self.sent_byte_size.fetch_add(byte_size, Ordering::Relaxed);
    }

    pub fn try_increment_dropped_event_count(&self, count: u64) {
        if let Some(dropped_event_count) = &self.dropped_event_count {
            dropped_event_count.fetch_add(count, Ordering::Relaxed);
        }
    }
}
