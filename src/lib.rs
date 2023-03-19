//! A set of thread-safe channels in Rust that conflates
//! submitted items via sender-supplied keys.
//!
//! ## Properties
//!
//! These channels function identically to their standard,
//! non-conflating counterparts if every submitted
//! key is unique and/or the channel's consumer(s)
//! are keeping up with the producer(s). Otherwise, if
//! a producer submits a message with a duplicated key,
//! the old message will be dropped and any subsequent
//! consumer(s) will not be able to read it.
//!
//! Messages will always be received in the order that
//! they were submitted within the context of a single
//! sender. Submission order across multiple senders
//! is indeterminate.
//!
//! ## Example
//!
//! ```rust
//! use conflation::unbounded;
//!
//! let (tx, rx) = unbounded();
//! // Both messages are tagged with key = 1
//! let tx_result1 = tx.send(1, "foo".to_owned()).unwrap();
//! let tx_result2 = tx.send(1, "bar".to_owned()).unwrap();
//! // The receiver will only yield (1, "bar") as (1, "foo") was
//! // conflated by the producer.
//! let (key, value) = rx.recv().unwrap();
//! ```

use parking_lot::Mutex;
use std::collections::VecDeque;
use std::hash::Hash;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

mod control;
mod signal;

use self::control::ControlBlock;
use self::queue::ConflatingQueue;

pub mod queue;
pub mod receiver;
pub mod sender;

pub use self::receiver::*;
pub use self::sender::*;

/// Create a conflating channel with no maximum capacity.
///
/// The subsequent [`Sender`] and [`Receiver`] that are returned
/// are thread-safe, cloneable, and may be moved between threads
/// freely. They are connected in that sending via the [`Sender`]
/// will result in the values being accessible through the
/// corresponding [`Receiver`], except in the event of conflation.
pub fn unbounded<K, V>() -> (Sender<K, V>, Receiver<K, V>)
where
    K: Eq,
    K: Hash,
{
    let control = Arc::new(Mutex::new(ControlBlock {
        queue: ConflatingQueue::new(),
        disconnected: false,
        consumers: VecDeque::new(),
    }));
    let tx = Sender {
        refcount: Arc::new(AtomicUsize::new(1)),
        control: control.clone(),
    };
    let rx = Receiver {
        refcount: Arc::new(AtomicUsize::new(1)),
        control: control,
    };
    (tx, rx)
}
