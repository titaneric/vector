use std::{error, fmt};

use bytes::{Buf, BufMut};
use tokio::sync::mpsc::Sender;

use crate::{
    bytes::{DecodeBytes, EncodeBytes},
    topology::{
        builder::IntoBuffer,
        channel::{BufferReceiver, BufferSender},
    },
    Bufferable,
};
use crate::{MemoryBuffer, WhenFull};

// Silly implementation of `EncodeBytes`/`DecodeBytes` to fulfill `Bufferable` for our test buffer code.
impl EncodeBytes<u64> for u64 {
    type Error = BasicError;

    fn encode<B>(self, buffer: &mut B) -> Result<(), Self::Error>
    where
        B: BufMut,
        Self: Sized,
    {
        buffer.put_u64(self);
        Ok(())
    }
}

impl DecodeBytes<u64> for u64 {
    type Error = BasicError;

    fn decode<B>(mut buffer: B) -> Result<u64, Self::Error>
    where
        B: Buf,
    {
        if buffer.remaining() >= 8 {
            Ok(buffer.get_u64())
        } else {
            Err(BasicError("need 8 bytes minimum".to_string()))
        }
    }
}

#[derive(Debug)]
pub struct BasicError(pub(crate) String);

impl fmt::Display for BasicError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl error::Error for BasicError {}

/// Builds a buffer using in-memory channels.
///
/// If `mode` is set to `WhenFull::Overflow`, then the buffer will be set to overflow mode, with
/// another in-memory channel buffer being used as the overflow buffer.  The overflow buffer will
/// also use the same capacity as the outer buffer.
pub fn build_buffer(
    capacity: usize,
    mode: WhenFull,
    overflow_mode: Option<WhenFull>,
) -> (BufferSender<u64>, BufferReceiver<u64>) {
    match mode {
        WhenFull::Block | WhenFull::DropNewest => {
            let channel = MemoryBuffer::new(capacity);
            let (sender, receiver) = channel.into_buffer_parts();
            let sender = BufferSender::new(sender, mode);
            let receiver = BufferReceiver::new(receiver);
            (sender, receiver)
        }
        WhenFull::Overflow => {
            let overflow_mode = overflow_mode
                .expect("overflow_mode must be specified when base is in overflow mode");
            let overflow_channel = MemoryBuffer::new(capacity);
            let (overflow_sender, overflow_receiver) = overflow_channel.into_buffer_parts();
            let overflow_sender = BufferSender::new(overflow_sender, overflow_mode);
            let overflow_receiver = BufferReceiver::new(overflow_receiver);

            let base_channel = MemoryBuffer::new(capacity);
            let (base_sender, base_receiver) = base_channel.into_buffer_parts();
            let base_sender = BufferSender::with_overflow(base_sender, overflow_sender);
            let base_receiver = BufferReceiver::with_overflow(base_receiver, overflow_receiver);

            (base_sender, base_receiver)
        }
    }
}

/// Gets the current capacity of the underlying base channel of the given sender.
fn get_base_sender_capacity<T: Bufferable>(sender: &BufferSender<T>) -> usize {
    sender
        .get_base_ref()
        .get_ref()
        .expect("channel should be live")
        .capacity()
}

/// Gets the current capacity of the underlying overflow channel of the given sender..
///
/// As overflow is optional, the return value will be `None` is overflow is not configured.
fn get_overflow_sender_capacity<T: Bufferable>(sender: &BufferSender<T>) -> Option<usize> {
    sender
        .get_overflow_ref()
        .and_then(|s| s.get_base_ref().get_ref())
        .map(Sender::capacity)
}

/// Asserts the given sender's capacity, both for base and overflow, match the given values.
///
/// The overflow value is wrapped in `Option<T>` as not all senders will have overflow configured.
#[allow(clippy::missing_panics_doc)]
pub fn assert_current_send_capacity<T>(
    sender: &mut BufferSender<T>,
    base_expected: usize,
    overflow_expected: Option<usize>,
) where
    T: Bufferable,
{
    assert_eq!(get_base_sender_capacity(sender), base_expected);
    assert_eq!(get_overflow_sender_capacity(sender), overflow_expected);
}
