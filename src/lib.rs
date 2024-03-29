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
use queue::ConflatingQueue;
use std::collections::VecDeque;
use std::hash::Hash;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

mod control;
mod signal;

use self::control::ControlBlock;
use linked_hash_map::LinkedHashMap;

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
///
/// The default underlying queue implementation is a linked hash map.
/// Duplicates are immediately removed upon insertion.
pub fn unbounded<'a, K, V>() -> (Sender<'a, K, V>, Receiver<'a, K, V>)
where
    K: Eq,
    K: Hash,
    K: 'a,
    V: 'a,
    K: Send,
    V: Send,
{
    unbounded_with_queue(Box::new(LinkedHashMap::new()))
}

/// Create a conflating channel with no maximum capacity.
///
/// The subsequent [`Sender`] and [`Receiver`] that are returned
/// are thread-safe, cloneable, and may be moved between threads
/// freely. They are connected in that sending via the [`Sender`]
/// will result in the values being accessible through the
/// corresponding [`Receiver`], except in the event of conflation.
///
/// The queue implementation is user-specified and should implement
/// the [`ConflatingQueue`] trait. Trait implementations for
/// [`VecDeque<(K, V)>`], [`LinkedHashMap<K, V>`], [`CompactingQueue<K, V>`],
/// have been provided already.
pub fn unbounded_with_queue<'a, K, V>(
    queue: Box<dyn ConflatingQueue<K, V> + 'a>,
) -> (Sender<'a, K, V>, Receiver<'a, K, V>)
where
    K: Eq,
    K: Hash,
    K: 'a,
    V: 'a,
    K: Send,
    V: Send,
{
    let control = Arc::new(Mutex::new(ControlBlock {
        queue: queue,
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
