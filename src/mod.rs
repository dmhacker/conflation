use linked_hash_map::LinkedHashMap;
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::hash::Hash;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

mod control;
mod signal;

use self::control::ControlBlock;

pub mod receiver;
pub mod sender;

pub use self::receiver::*;
pub use self::sender::*;

pub fn unbounded<K, V>() -> (Sender<K, V>, Receiver<K, V>)
where
    K: Eq,
    K: Hash,
{
    let control = Arc::new(Mutex::new(ControlBlock {
        queue: LinkedHashMap::new(),
        disconnected: false,
        unreserved_count: 0,
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
