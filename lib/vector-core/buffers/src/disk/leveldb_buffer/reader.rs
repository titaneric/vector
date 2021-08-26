use super::Key;
use crate::bytes::DecodeBytes;
use bytes::Bytes;
use futures::{task::AtomicWaker, Stream};
use leveldb::database::{
    batch::{Batch, Writebatch},
    compaction::Compaction,
    iterator::{Iterable, LevelDBIterator},
    options::{ReadOptions, WriteOptions},
    Database,
};
use std::collections::VecDeque;
use std::fmt::Display;
use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};
use std::task::{Context, Poll, Waker};
use std::time::{Duration, Instant};

/// How much time needs to pass between compaction to trigger new one.
const MIN_TIME_UNCOMPACTED: Duration = Duration::from_secs(60);

/// Minimal size of uncompacted for which a compaction can be triggered.
const MIN_UNCOMPACTED_SIZE: usize = 4 * 1024 * 1024;

/// The reader side of N to 1 channel through leveldb.
///
/// Reader maintains/manages events thorugh several stages.
/// Unread -> Read -> Deleted -> Compacted
///
/// So the disk buffer (indices/keys) is separated into following regions.
/// |--Compacted--|--Deleted--|--Read--|--Unread
///  ^             ^   ^       ^        ^
///  |             |   |-acked-|        |
///  0   `compacted_offset`    |        |
///                     `delete_offset` |
///                                `read_offset`
pub struct Reader<T>
where
    T: Send + Sync + Unpin,
{
    /// Leveldb database.
    /// Shared with Writers.
    pub(crate) db: Arc<Database<Key>>,
    /// First unread key
    pub(crate) read_offset: usize,
    /// First uncompacted key
    pub(crate) compacted_offset: usize,
    /// First not deleted key
    pub(crate) delete_offset: usize,
    /// Number of acked events that haven't been deleted from
    /// database. Used for batching deletes.
    pub(crate) acked: usize,
    /// Reader is notified by Writers through this Waker.
    /// Shared with Writers.
    pub(crate) write_notifier: Arc<AtomicWaker>,
    /// Writers blocked by disk being full.
    /// Shared with Writers.
    pub(crate) blocked_write_tasks: Arc<Mutex<Vec<Waker>>>,
    /// Size of unread events in bytes.
    /// Shared with Writers.
    pub(crate) current_size: Arc<AtomicUsize>,
    /// Number of oldest read, not deleted, events that have been acked by the consumer.
    /// Shared with consumer.
    pub(crate) ack_counter: Arc<AtomicUsize>,
    /// Size of deleted, not compacted, events in bytes.
    pub(crate) uncompacted_size: usize,
    /// Sizes in bytes of read, not acked/deleted, events.
    pub(crate) unacked_sizes: VecDeque<usize>,
    /// Buffer for internal use.
    pub(crate) buffer: VecDeque<(Key, Vec<u8>)>,
    /// Limit on uncompacted_size after which we trigger compaction.
    pub(crate) max_uncompacted_size: usize,
    /// Last time that compaction was triggered.
    pub(crate) last_compaction: Instant,
    pub(crate) phantom: PhantomData<T>,
}

// Writebatch isn't Send, but the leveldb docs explicitly say that it's okay to
// share across threads
unsafe impl<T> Send for Reader<T> where T: Send + Sync + Unpin {}

impl<T> Stream for Reader<T>
where
    T: Send + Sync + Unpin + DecodeBytes<T>,
    <T as DecodeBytes<T>>::Error: Display,
{
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();

        // If there's no value at read_offset, we return NotReady and rely on
        // Writer using write_notifier to wake this task up after the next
        // write.
        this.write_notifier.register(cx.waker());

        let unread_size = this.delete_acked();

        if this.acked >= 100 {
            this.flush(unread_size);
        }

        if this.buffer.is_empty() {
            // This will usually complete instantly, but in the case of a large
            // queue (or a fresh launch of the app), this will have to go to
            // disk.
            tokio::task::block_in_place(|| {
                this.buffer.extend(
                    this.db
                        .iter(ReadOptions::new())
                        .from(&Key(this.read_offset))
                        .take(100),
                );
            });
        }

        if let Some((key, value)) = this.buffer.pop_front() {
            this.unacked_sizes.push_back(value.len());
            this.read_offset = key.0 + 1;

            let buffer: Bytes = Bytes::from(value);
            match T::decode(buffer) {
                Ok(event) => Poll::Ready(Some(event)),
                Err(error) => {
                    error!(message = "Error deserializing event.", %error);
                    debug_assert!(false);
                    Pin::new(this).poll_next(cx)
                }
            }
        } else if Arc::strong_count(&this.db) == 1 {
            // There are no writers left
            Poll::Ready(None)
        } else {
            Poll::Pending
        }
    }
}

impl<T> Drop for Reader<T>
where
    T: Send + Sync + Unpin,
{
    fn drop(&mut self) {
        let unread_size = self.delete_acked();
        self.flush(unread_size);
    }
}

impl<T> Reader<T>
where
    T: Send + Sync + Unpin,
{
    /// Returns number of bytes to be read.
    fn delete_acked(&mut self) -> usize {
        let num_to_delete = self.ack_counter.swap(0, Ordering::Relaxed);

        let unread_size = if num_to_delete > 0 {
            let size_deleted = self.unacked_sizes.drain(..num_to_delete).sum();
            let unread_size =
                self.current_size.fetch_sub(size_deleted, Ordering::Release) - size_deleted;

            self.uncompacted_size += size_deleted;
            self.acked += num_to_delete;

            unread_size
        } else {
            self.current_size.load(Ordering::Acquire)
        };

        for task in self.blocked_write_tasks.lock().unwrap().drain(..) {
            task.wake();
        }

        unread_size
    }

    fn flush(&mut self, unread_size: usize) {
        if self.acked > 0 {
            let new_offset = self.delete_offset + self.acked;
            assert!(
                new_offset <= self.read_offset,
                "Tried to ack beyond read offset"
            );

            let mut delete_batch = Writebatch::new();

            for i in self.delete_offset..new_offset {
                delete_batch.delete(Key(i));
            }

            self.db.write(WriteOptions::new(), &delete_batch).unwrap();

            self.delete_offset = new_offset;
            self.acked = 0;

            // Compaction can be triggered in two ways:
            //  1. When size of uncompacted is a percentage of total allowed size.
            //     Managed by MAX_UNCOMPACTED. This is to limit the size of disk buffer
            //     under configured max size.
            let max_trigger = self.uncompacted_size > self.max_uncompacted_size;
            //  2. When the size of uncompacted buffer is larger than unread buffer.
            //     If the sink is able to keep up with the sources, this will trigger
            //     with MIN_TIME_UNCOMPACTED interval. And if it's not keeping up,
            //     this won't trigger hence it won't slow it down, which will allow it
            //     to grow until it either hits max_uncompacted_size or manages to catch up.
            //     This is to keep the size of the disk buffer low in idle and up to date
            //     cases.
            let timed_trigger = self.last_compaction.elapsed() >= MIN_TIME_UNCOMPACTED
                && self.uncompacted_size > unread_size;

            // Basic requirement to avoid leaving ldb files behind.
            // See:
            // Vector  https://github.com/timberio/vector/issues/7425#issuecomment-849522738
            // leveldb https://github.com/google/leveldb/issues/783
            //         https://github.com/syndtr/goleveldb/issues/166
            let min_size = self.uncompacted_size >= MIN_UNCOMPACTED_SIZE;

            if min_size && (max_trigger || timed_trigger) {
                self.compact();
            }
        }
    }

    pub(crate) fn compact(&mut self) {
        if self.uncompacted_size > 0 {
            self.uncompacted_size = 0;

            debug!("Compacting disk buffer.");
            self.db
                .compact(&Key(self.compacted_offset), &Key(self.delete_offset));

            self.compacted_offset = self.delete_offset;
        }
        self.last_compaction = Instant::now();
    }
}
