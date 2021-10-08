use super::EncodedEvent;
use crate::{event::EventFinalizers, internal_events::LargeEventDropped};
use derivative::Derivative;
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use std::marker::PhantomData;
use std::num::NonZeroUsize;
use std::time::Duration;
use vector_core::stream::BatcherSettings;

#[derive(Debug, Snafu, PartialEq)]
pub enum BatchError {
    #[snafu(display("This sink does not allow setting `max_bytes`"))]
    BytesNotAllowed,
    #[snafu(display("`max_bytes` was unexpectedly zero"))]
    InvalidMaxBytes,
    #[snafu(display("`max_events` was unexpectedly zero"))]
    InvalidMaxEvents,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
pub struct BatchConfig {
    pub max_bytes: Option<usize>,
    pub max_events: Option<usize>,
    pub timeout_secs: Option<u64>,
}

impl BatchConfig {
    pub const fn disallow_max_bytes(&self) -> Result<Self, BatchError> {
        // Sinks that used `max_size` for an event count cannot count
        // bytes, so err if `max_bytes` is set.
        match self.max_bytes {
            Some(_) => Err(BatchError::BytesNotAllowed),
            None => Ok(*self),
        }
    }

    pub const fn limit_max_bytes(self, limit: usize) -> Self {
        if let Some(n) = self.max_bytes {
            if n > limit {
                return Self {
                    max_bytes: Some(limit),
                    ..self
                };
            }
        }

        self
    }

    pub const fn limit_max_events(self, limit: usize) -> Self {
        if let Some(n) = self.max_events {
            if n > limit {
                return Self {
                    max_events: Some(limit),
                    ..self
                };
            }
        }

        self
    }

    pub(super) fn get_settings_or_default<T>(
        &self,
        defaults: BatchSettings<T>,
    ) -> BatchSettings<T> {
        BatchSettings {
            size: BatchSize {
                bytes: self.max_bytes.unwrap_or(defaults.size.bytes),
                events: self.max_events.unwrap_or(defaults.size.events),
                ..Default::default()
            },
            timeout: self
                .timeout_secs
                .map(Duration::from_secs)
                .unwrap_or(defaults.timeout),
        }
    }
}

#[derive(Debug, Derivative)]
#[derivative(Clone(bound = ""))]
#[derivative(Copy(bound = ""))]
pub struct BatchSize<B> {
    pub bytes: usize,
    pub events: usize,
    // This type marker is used to drive type inference, which allows us
    // to call the right Batch::get_settings_defaults without explicitly
    // naming the type in BatchSettings::parse_config.
    _type_marker: PhantomData<B>,
}

impl<B> BatchSize<B> {
    pub const fn const_default() -> Self {
        BatchSize {
            bytes: usize::max_value(),
            events: usize::max_value(),
            _type_marker: PhantomData,
        }
    }
}

impl<B> Default for BatchSize<B> {
    fn default() -> Self {
        BatchSize::const_default()
    }
}

#[derive(Debug, Derivative)]
#[derivative(Clone(bound = ""))]
#[derivative(Copy(bound = ""))]
pub struct BatchSettings<B> {
    pub size: BatchSize<B>,
    pub timeout: Duration,
}

impl<B: Batch> BatchSettings<B> {
    pub fn parse_config(self, config: BatchConfig) -> Result<Self, BatchError> {
        B::get_settings_defaults(config, self)
    }
}

impl<B> BatchSettings<B> {
    pub const fn const_default() -> Self {
        BatchSettings {
            size: BatchSize::const_default(),
            timeout: Duration::from_nanos(0),
        }
    }

    // Fake the builder pattern
    pub const fn bytes(self, bytes: usize) -> Self {
        Self {
            size: BatchSize {
                bytes: bytes as usize,
                ..self.size
            },
            ..self
        }
    }
    pub const fn events(self, events: usize) -> Self {
        Self {
            size: BatchSize {
                events,
                ..self.size
            },
            ..self
        }
    }
    pub const fn timeout(self, secs: u64) -> Self {
        Self {
            timeout: Duration::from_secs(secs),
            ..self
        }
    }

    // Would like to use `trait From` here, but that results in
    // "conflicting implementations of trait"
    pub const fn into<B2>(self) -> BatchSettings<B2> {
        BatchSettings {
            size: BatchSize {
                bytes: self.size.bytes,
                events: self.size.events,
                _type_marker: PhantomData,
            },
            timeout: self.timeout,
        }
    }

    /// Converts these settings into [`BatcherSettings`].
    ///
    /// `BatcherSettings` is effectively the `vector_core` spiritual successor of
    /// [`BatchSettings<B>`].  Once all sinks are rewritten in the new stream-based style and we can
    /// eschew customized batch buffer types, we can de-genericify `BatchSettings` and move it into
    /// `vector_core`, and use that instead of `BatcherSettings`.
    pub fn into_batcher_settings(self) -> Result<BatcherSettings, BatchError> {
        let max_bytes = Some(self.size.bytes)
            .map(|n| if n == 0 { usize::MAX } else { n })
            .and_then(NonZeroUsize::new)
            .ok_or(BatchError::InvalidMaxBytes)?;

        let max_events = Some(self.size.events)
            .map(|n| if n == 0 { usize::MAX } else { n })
            .and_then(NonZeroUsize::new)
            .ok_or(BatchError::InvalidMaxBytes)?;

        Ok(BatcherSettings::new(self.timeout, max_bytes, max_events))
    }
}

impl<B> Default for BatchSettings<B> {
    fn default() -> Self {
        BatchSettings::const_default()
    }
}

pub(super) fn err_event_too_large<T>(length: usize, max_length: usize) -> PushResult<T> {
    emit!(&LargeEventDropped { length, max_length });
    PushResult::Ok(false)
}

/// This enum provides the result of a push operation, indicating if the
/// event was added and the fullness state of the buffer.
#[must_use]
#[derive(Debug, Eq, PartialEq)]
pub enum PushResult<T> {
    /// Event was added, with an indicator if the buffer is now full
    Ok(bool),
    /// Event could not be added because it would overflow the
    /// buffer. Since push takes ownership of the event, it must be
    /// returned here.
    Overflow(T),
}

pub trait Batch: Sized {
    type Input;
    type Output;

    /// Turn the batch configuration into an actualized set of settings,
    /// and deal with the proper behavior of `max_size` and if
    /// `max_bytes` may be set. This is in the trait to ensure all batch
    /// buffers implement it.
    fn get_settings_defaults(
        _config: BatchConfig,
        _defaults: BatchSettings<Self>,
    ) -> Result<BatchSettings<Self>, BatchError>;

    fn push(&mut self, item: Self::Input) -> PushResult<Self::Input>;
    fn is_empty(&self) -> bool;
    fn fresh(&self) -> Self;
    fn finish(self) -> Self::Output;
    fn num_items(&self) -> usize;
}

/// This is a batch construct that stores an set of event finalizers alongside the batch itself.
#[derive(Clone, Debug)]
pub struct FinalizersBatch<B> {
    inner: B,
    finalizers: EventFinalizers,
}

impl<B: Batch> From<B> for FinalizersBatch<B> {
    fn from(inner: B) -> Self {
        Self {
            inner,
            finalizers: Default::default(),
        }
    }
}

impl<B: Batch> Batch for FinalizersBatch<B> {
    type Input = EncodedEvent<B::Input>;
    type Output = (B::Output, EventFinalizers);

    fn get_settings_defaults(
        config: BatchConfig,
        defaults: BatchSettings<Self>,
    ) -> Result<BatchSettings<Self>, BatchError> {
        Ok(B::get_settings_defaults(config, defaults.into())?.into())
    }

    fn push(&mut self, item: Self::Input) -> PushResult<Self::Input> {
        let EncodedEvent { item, finalizers } = item;
        match self.inner.push(item) {
            PushResult::Ok(full) => {
                self.finalizers.merge(finalizers);
                PushResult::Ok(full)
            }
            PushResult::Overflow(item) => PushResult::Overflow(EncodedEvent { item, finalizers }),
        }
    }

    fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    fn fresh(&self) -> Self {
        Self {
            inner: self.inner.fresh(),
            finalizers: Default::default(),
        }
    }

    fn finish(self) -> Self::Output {
        (self.inner.finish(), self.finalizers)
    }

    fn num_items(&self) -> usize {
        self.inner.num_items()
    }
}

#[derive(Clone, Debug)]
pub struct StatefulBatch<B> {
    inner: B,
    was_full: bool,
}

impl<B: Batch> From<B> for StatefulBatch<B> {
    fn from(inner: B) -> Self {
        Self {
            inner,
            was_full: false,
        }
    }
}

impl<B> StatefulBatch<B> {
    pub const fn was_full(&self) -> bool {
        self.was_full
    }

    #[allow(clippy::missing_const_for_fn)] // const cannot run destructor
    pub fn into_inner(self) -> B {
        self.inner
    }
}

impl<B: Batch> Batch for StatefulBatch<B> {
    type Input = B::Input;
    type Output = B::Output;

    fn get_settings_defaults(
        config: BatchConfig,
        defaults: BatchSettings<Self>,
    ) -> Result<BatchSettings<Self>, BatchError> {
        Ok(B::get_settings_defaults(config, defaults.into())?.into())
    }

    fn push(&mut self, item: Self::Input) -> PushResult<Self::Input> {
        if self.was_full {
            PushResult::Overflow(item)
        } else {
            let result = self.inner.push(item);
            self.was_full =
                matches!(result, PushResult::Overflow(_)) || matches!(result, PushResult::Ok(true));
            result
        }
    }

    fn is_empty(&self) -> bool {
        !self.was_full && self.inner.is_empty()
    }

    fn fresh(&self) -> Self {
        Self {
            inner: self.inner.fresh(),
            was_full: false,
        }
    }

    fn finish(self) -> Self::Output {
        self.inner.finish()
    }

    fn num_items(&self) -> usize {
        self.inner.num_items()
    }
}

impl Batch for () {
    type Input = ();
    type Output = ();

    fn get_settings_defaults(
        config: BatchConfig,
        defaults: BatchSettings<Self>,
    ) -> Result<BatchSettings<Self>, BatchError> {
        Ok(config.get_settings_or_default(defaults))
    }

    fn push(&mut self, _item: Self::Input) -> PushResult<Self::Input> {
        PushResult::Ok(false)
    }

    fn is_empty(&self) -> bool {
        true
    }

    fn fresh(&self) -> Self {}

    fn finish(self) -> Self::Output {}

    fn num_items(&self) -> usize {
        0
    }
}
