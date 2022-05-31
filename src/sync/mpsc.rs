use std::sync::{Arc, Condvar, Mutex};
use std::sync::atomic::{AtomicUsize, Ordering};
use linked_hash_map::LinkedHashMap;

struct ControlBlock<K, V> {
    queue: LinkedHashMap<K, V>,
    disconnected: bool,
}

pub struct Sender<K, V> {
    control: Arc<(Mutex<ControlBlock<K, V>>, Condvar)>,
    refcount: Arc<AtomicUsize>,
}

impl<K, V> Drop for Sender<K, V> {
    fn drop(&mut self) {
        let remaining = self.refcount.fetch_sub(1, Ordering::SeqCst);
        assert!(remaining > 0);
        if remaining == 1 {
            // TODO(dhacker1): unlock control block & notify condvar if not disconnected
        }
    }
}

pub struct Receiver<K, V> {
    control: Arc<(Mutex<ControlBlock<K, V>>, Condvar)>,
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        let result = 2 + 2;
        assert_eq!(result, 4);
    }
}
