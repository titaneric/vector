use crate::{buffers::EventStream, event::Event, stats};
use futures::{Stream, StreamExt};
use pin_project::pin_project;
use std::{pin::Pin, task::Context};
use std::{
    task::Poll,
    time::{Duration, Instant},
};
use tokio::time::interval;
use tokio_stream::wrappers::IntervalStream;

#[pin_project]
struct Utilization {
    timer: Timer,
    intervals: IntervalStream,
    inner: Pin<EventStream>,
}

impl Stream for Utilization {
    type Item = Event;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // The goal of this function is to measure the time between when the
        // caller requests the next Event from the stream and before one is
        // ready, with the side-effect of reporting every so often about how
        // long the wait gap is.
        //
        // To achieve this we poll the `intervals` stream and if a new interval
        // is ready we hit `Timer::report` and loop back around again to poll
        // for a new `Event`. Calls to `Timer::start_wait` will only have an
        // effect if `stop_wait` has been called, so the structure of this loop
        // avoids double-measures.
        let this = self.project();
        loop {
            this.timer.start_wait();
            match this.intervals.poll_next_unpin(cx) {
                Poll::Ready(_) => {
                    this.timer.report();
                    continue;
                }
                Poll::Pending => match this.inner.poll_next_unpin(cx) {
                    pe @ Poll::Ready(_) => {
                        this.timer.stop_wait();
                        return pe;
                    }
                    Poll::Pending => return Poll::Pending,
                },
            }
        }
    }
}

/// Wrap a stream to emit stats about utilization. This is designed for use with
/// the input channels of transform and sinks components, and measures the
/// amount of time that the stream is waiting for input from upstream. We make
/// the simplifying assumption that this wait time is when the component is idle
/// and the rest of the time it is doing useful work. This is more true for
/// sinks than transforms, which can be blocked by downstream components, but
/// with knowledge of the config the data is still useful.
pub fn wrap(inner: Pin<EventStream>) -> Pin<EventStream> {
    let utilization = Utilization {
        timer: Timer::new(),
        intervals: IntervalStream::new(interval(Duration::from_secs(5))),
        inner,
    };

    Box::pin(utilization)
}

struct Timer {
    overall_start: Instant,
    span_start: Instant,
    waiting: bool,
    total_wait: Duration,
    ewma: stats::Ewma,
}

/// A simple, specialized timer for tracking spans of waiting vs not-waiting
/// time and reporting a smoothed estimate of utilization.
///
/// This implementation uses the idea of spans and reporting periods. Spans are
/// a period of time spent entirely in one state, aligning with state
/// transitions but potentially more granular.  Reporting periods are expected
/// to be of uniform length and used to aggregate span data into time-weighted
/// averages.
impl Timer {
    fn new() -> Self {
        Self {
            overall_start: Instant::now(),
            span_start: Instant::now(),
            waiting: false,
            total_wait: Duration::new(0, 0),
            ewma: stats::Ewma::new(0.9),
        }
    }

    /// Begin a new span representing time spent waiting
    fn start_wait(&mut self) {
        if !self.waiting {
            self.end_span();
            self.waiting = true;
        }
    }

    /// Complete the current waiting span and begin a non-waiting span
    fn stop_wait(&mut self) {
        assert!(self.waiting);

        self.end_span();
        self.waiting = false;
    }

    /// Meant to be called on a regular interval, this method calculates wait
    /// ratio since the last time it was called and reports the resulting
    /// utilization average.
    fn report(&mut self) {
        // End the current span so it can be accounted for, but do not change
        // whether or not we're in the waiting state. This way the next span
        // inherits the correct status.
        self.end_span();

        let total_duration = self.overall_start.elapsed();
        let wait_ratio = self.total_wait.as_secs_f64() / total_duration.as_secs_f64();
        let utilization = 1.0 - wait_ratio;

        self.ewma.update(utilization);
        debug!(utilization = %self.ewma.average().unwrap_or(f64::NAN));

        // Reset overall statistics for the next reporting period.
        self.overall_start = self.span_start;
        self.total_wait = Duration::new(0, 0);
    }

    fn end_span(&mut self) {
        if self.waiting {
            self.total_wait += self.span_start.elapsed();
        }
        self.span_start = Instant::now();
    }
}
