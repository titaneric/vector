use crate::ByteSizeOf;
use futures::stream::{Fuse, Stream};
use futures::{Future, StreamExt};
use pin_project::pin_project;

use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use crate::stream::BatcherSettings;
use tokio::time::Sleep;

pub trait ItemBatchSize<T> {
    /// The size of an individual item in a batch.
    fn size(&self, item: &T) -> usize;
}

pub struct ByteSizeOfItemSize;

impl<T: ByteSizeOf> ItemBatchSize<T> for ByteSizeOfItemSize {
    fn size(&self, item: &T) -> usize {
        item.size_of()
    }
}

impl<T, F> ItemBatchSize<T> for F
where
    F: Fn(&T) -> usize,
{
    fn size(&self, item: &T) -> usize {
        (self)(item)
    }
}

#[pin_project]
pub struct Batcher<S, I>
where
    S: Stream,
{
    /// The total "size" of all items in a batch. Size is intentionally
    /// vague here since it is user defined, and can vary.
    ///
    /// To ensure any individual event can be placed in a batch, the first element in a batch is not
    /// subject to this limit.
    batch_size_limit: usize,

    /// Total number of items that will be placed in a single batch.
    ///
    /// To ensure any individual event can be placed in a batch, the first element in a batch is not
    /// subject to this limit.
    batch_item_limit: usize,

    current_size: usize,

    batch: Vec<S::Item>,

    #[pin]
    /// The stream this `Batcher` wraps
    stream: Fuse<S>,

    /// A function to calculate the size of an individual item
    item_batch_size: I,

    #[pin]
    timer: Maybe<Sleep>,
    timeout: Duration,
}

/// An `Option`, but with pin projection
#[pin_project(project = MaybeProj)]
pub enum Maybe<T> {
    Some(#[pin] T),
    None,
}

impl<S, I> Batcher<S, I>
where
    S: Stream,
    I: ItemBatchSize<S::Item>,
{
    pub fn new(stream: S, settings: BatcherSettings, batch_size_calculator: I) -> Self {
        Self {
            batch_size_limit: settings.size_limit,
            batch_item_limit: settings.item_limit,
            current_size: 0,
            batch: vec![],
            stream: stream.fuse(),
            item_batch_size: batch_size_calculator,
            timer: Maybe::None,
            timeout: settings.timeout,
        }
    }
}

impl<S, I> Batcher<S, I>
where
    S: Stream,
    I: ItemBatchSize<S::Item>,
{
    fn size_fits_in_batch(&self, item_size: usize) -> bool {
        if self.batch.is_empty() {
            // make sure any individual item can always fit in a batch
            return true;
        }
        self.current_size + item_size <= self.batch_size_limit
    }

    /// returns true iff it is not possible for another item to fit in the batch
    fn is_batch_full(&self) -> bool {
        self.batch.len() >= self.batch_item_limit || self.current_size >= self.batch_size_limit
    }

    fn take_batch(self: Pin<&mut Self>) -> Vec<S::Item> {
        let mut this = self.project();
        *this.current_size = 0;
        this.timer.set(Maybe::None);
        std::mem::take(this.batch)
    }

    fn push_item(self: Pin<&mut Self>, item: S::Item, item_size: usize) {
        let this = self.project();
        this.batch.push(item);
        *this.current_size += item_size;
    }

    fn start_timer(self: Pin<&mut Self>) {
        let timeout = self.timeout;
        self.project()
            .timer
            .set(Maybe::Some(tokio::time::sleep(timeout)));
    }
}

impl<S, I> Stream for Batcher<S, I>
where
    S: Stream,
    I: ItemBatchSize<S::Item>,
{
    type Item = Vec<S::Item>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            match self.as_mut().project().stream.poll_next(cx) {
                Poll::Ready(None) => {
                    return {
                        if self.batch.is_empty() {
                            Poll::Ready(None)
                        } else {
                            Poll::Ready(Some(self.take_batch()))
                        }
                    }
                }
                Poll::Ready(Some(item)) => {
                    let item_size = self.item_batch_size.size(&item);
                    if self.size_fits_in_batch(item_size) {
                        self.as_mut().push_item(item, item_size);
                        if self.is_batch_full() {
                            return Poll::Ready(Some(self.as_mut().take_batch()));
                        } else if self.batch.len() == 1 {
                            self.as_mut().start_timer();
                        }
                    } else {
                        let output = Poll::Ready(Some(self.as_mut().take_batch()));
                        self.as_mut().push_item(item, item_size);
                        self.as_mut().start_timer();
                        return output;
                    }
                }
                Poll::Pending => {
                    return {
                        if let MaybeProj::Some(timer) = self.as_mut().project().timer.project() {
                            match timer.poll(cx) {
                                Poll::Ready(()) => {
                                    self.as_mut().project().timer.set(Maybe::None);
                                    debug_assert!(
                                        !self.batch.is_empty(),
                                        "timer should have been cancelled"
                                    );
                                    Poll::Ready(Some(self.take_batch()))
                                }
                                Poll::Pending => Poll::Pending,
                            }
                        } else {
                            Poll::Pending
                        }
                    }
                }
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.stream.size_hint()
    }
}

#[cfg(test)]
#[allow(clippy::similar_names)]
mod test {
    use super::*;
    use futures::stream;
    use std::num::NonZeroUsize;

    #[tokio::test]
    async fn item_limit() {
        let stream = stream::iter([1, 2, 3]);
        let settings = BatcherSettings::new(
            Duration::from_millis(100),
            NonZeroUsize::new(10000).unwrap(),
            NonZeroUsize::new(2).unwrap(),
        );
        let batcher = Batcher::new(stream, settings, |x: &u32| *x as usize);
        let batches: Vec<_> = batcher.collect().await;
        assert_eq!(batches, vec![vec![1, 2], vec![3],]);
    }

    #[tokio::test]
    async fn size_limit() {
        let batcher = Batcher::new(
            stream::iter([1, 2, 3, 4, 5, 6, 2, 3, 1]),
            BatcherSettings::new(
                Duration::from_millis(100),
                NonZeroUsize::new(5).unwrap(),
                NonZeroUsize::new(100).unwrap(),
            ),
            |x: &u32| *x as usize,
        );
        let batches: Vec<_> = batcher.collect().await;
        assert_eq!(
            batches,
            vec![
                vec![1, 2],
                vec![3],
                vec![4],
                vec![5],
                vec![6],
                vec![2, 3],
                vec![1],
            ]
        );
    }

    #[tokio::test]
    async fn timeout_limit() {
        tokio::time::pause();

        let timeout = Duration::from_millis(100);
        let stream = stream::iter([1, 2]).chain(stream::pending());
        let batcher = Batcher::new(
            stream,
            BatcherSettings::new(
                timeout,
                NonZeroUsize::new(5).unwrap(),
                NonZeroUsize::new(100).unwrap(),
            ),
            |x: &u32| *x as usize,
        );

        tokio::pin!(batcher);
        let mut next = batcher.next();
        assert_eq!(futures::poll!(&mut next), Poll::Pending);
        tokio::time::advance(timeout).await;
        let batch = next.await;
        assert_eq!(batch, Some(vec![1, 2]));
    }
}
