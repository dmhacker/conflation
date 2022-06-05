use linked_hash_map::LinkedHashMap;
use std::hash::Hash;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{RecvError, RecvTimeoutError, SendError, TryRecvError};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

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
        let remaining = self.refcount.fetch_sub(1, Ordering::SeqCst) - 1;
        if remaining == 0 {
            let (lock, cvar) = &*self.control;
            let mut control_guard = lock.lock().unwrap();
            // Receiver could have already disconnected
            if !control_guard.disconnected {
                control_guard.disconnected = true;
                cvar.notify_one();
            }
        }
    }
}

impl<K, V> Clone for Sender<K, V> {
    fn clone(&self) -> Sender<K, V> {
        self.refcount.fetch_add(1, Ordering::SeqCst);
        Sender {
            control: self.control.clone(),
            refcount: self.refcount.clone()
        }
    }
}

impl<K, V> Sender<K, V>
where
    K: Eq,
    K: Hash,
{
    pub fn send(&mut self, key: K, value: V) -> Result<(), SendError<(K, V)>> {
        let (lock, cvar) = &*self.control;
        let mut control_guard = lock.lock().unwrap();
        if control_guard.disconnected {
            return Err(SendError((key, value)));
        }
        control_guard.queue.insert(key, value);
        cvar.notify_one();
        Ok(())
    }
}

pub struct Receiver<K, V> {
    control: Arc<(Mutex<ControlBlock<K, V>>, Condvar)>,
}

impl<K, V> Drop for Receiver<K, V> {
    fn drop(&mut self) {
        let (lock, _) = &*self.control;
        let mut control_guard = lock.lock().unwrap();
        control_guard.disconnected = true;
    }
}

impl<K, V> Receiver<K, V>
where
    K: Eq,
    K: Hash,
{
    pub fn try_recv(&mut self) -> Result<(K, V), TryRecvError> {
        let (lock, _) = &*self.control;
        let mut control_guard = lock.lock().unwrap();
        match control_guard.queue.pop_front() {
            Some(head) => Ok(head),
            None => {
                if control_guard.disconnected {
                    Err(TryRecvError::Disconnected)
                } else {
                    Err(TryRecvError::Empty)
                }
            }
        }
    }

    pub fn recv(&mut self) -> Result<(K, V), RecvError> {
        let (lock, cvar) = &*self.control;
        let mut control_guard = lock.lock().unwrap();
        // The queue should be checked before the disconnect flag as
        // a receiver is still allowed to drain a disconnected queue.
        if let Some(head) = control_guard.queue.pop_front() {
            return Ok(head);
        }
        if control_guard.disconnected {
            return Err(RecvError);
        }
        control_guard = cvar
            .wait_while(control_guard, |control| {
                !control.disconnected && control.queue.is_empty()
            })
            .unwrap();
        // After a wake-up, the above still applies; the queue should be
        // drained before a disconnect is reported
        match control_guard.queue.pop_front() {
            Some(head) => Ok(head),
            None => {
                if control_guard.disconnected {
                    return Err(RecvError);
                } else {
                    panic!("wakeup invariant failed: receiver was not disconnected and queue was empty")
                }
            }
        }
    }

    pub fn recv_timeout(&mut self, timeout: Duration) -> Result<(K, V), RecvTimeoutError> {
        let (lock, cvar) = &*self.control;
        let mut control_guard = lock.lock().unwrap();
        // The queue should be checked before the disconnect flag as
        // a receiver is still allowed to drain a disconnected queue.
        if let Some(head) = control_guard.queue.pop_front() {
            return Ok(head);
        }
        if control_guard.disconnected {
            return Err(RecvTimeoutError::Disconnected);
        }
        let wait_result = cvar
            .wait_timeout_while(control_guard, timeout, |control| {
                !control.disconnected && control.queue.is_empty()
            })
            .unwrap();
        if wait_result.1.timed_out() {
            return Err(RecvTimeoutError::Timeout);
        }
        control_guard = wait_result.0;
        // After a wake-up, the above still applies; the queue should be
        // drained before a disconnect is reported
        match control_guard.queue.pop_front() {
            Some(head) => Ok(head),
            None => {
                if control_guard.disconnected {
                    return Err(RecvTimeoutError::Disconnected);
                } else {
                    panic!("wakeup invariant failed: receiver was not disconnected and queue was empty")
                }
            }
        }
    }
}

pub fn channel<K, V>() -> (Sender<K, V>, Receiver<K, V>)
where
    K: Eq,
    K: Hash,
{
    let control = ControlBlock::<K, V> {
        queue: LinkedHashMap::<K, V>::new(),
        disconnected: false,
    };
    let control_arc = Arc::new((Mutex::new(control), Condvar::new()));
    let tx = Sender::<K, V> {
        control: control_arc.clone(),
        refcount: Arc::new(AtomicUsize::new(1)),
    };
    let rx = Receiver::<K, V> {
        control: control_arc,
    };
    (tx, rx)
}

#[cfg(test)]
mod tests {
    use crate::sync::mpsc::channel;
    use std::collections::HashMap;
    use std::sync::mpsc::{RecvError, RecvTimeoutError, SendError, TryRecvError};
    use std::time::Duration;

    #[test]
    fn recv_has_no_conflations() {
        let (mut tx, mut rx) = channel();
        // Simple send and recv's
        assert_eq!(tx.send(1, "test 1".to_owned()), Ok(()));
        assert_eq!(rx.recv(), Ok((1, "test 1".to_owned())));
        assert_eq!(tx.send(2, "test 2".to_owned()), Ok(()));
        assert_eq!(rx.recv(), Ok((2, "test 2".to_owned())));
        // Send two at once
        assert_eq!(tx.send(8888, "foobar".to_owned()), Ok(()));
        assert_eq!(tx.send(9999, "amazing".to_owned()), Ok(()));
        assert_eq!(rx.recv(), Ok((8888, "foobar".to_owned())));
        assert_eq!(rx.recv(), Ok((9999, "amazing".to_owned())));
    }

    #[test]
    fn try_recv_has_single_conflation() {
        let (mut tx, mut rx) = channel();
        // All insertions correspond to a single key
        for i in 0..1000 {
            assert_eq!(tx.send(1337, format!("test {i}").to_owned()), Ok(()));
        }
        let mut received = HashMap::new();
        let mut counts = HashMap::new();
        loop {
            match rx.try_recv() {
                Ok((key, value)) => {
                    received.insert(key.clone(), value);
                    let counter = counts.entry(key).or_insert(0);
                    *counter += 1;
                }
                Err(TryRecvError::Empty) => {
                    break;
                }
                Err(TryRecvError::Disconnected) => {
                    assert!(false);
                }
            }
        }
        assert_eq!(received.len(), 1);
        assert_eq!(received.get(&1337), Some(&"test 999".to_owned()));
        assert_eq!(counts.len(), 1);
        assert_eq!(counts.get(&1337), Some(&1));
    }

    #[test]
    fn try_recv_has_many_conflations() {
        let (mut tx, mut rx) = channel();
        // Add duplicate insertions for each index
        for j in 0..4 {
            for i in 0..1000 {
                assert_eq!(tx.send(i, format!("test {i} {j}").to_owned()), Ok(()));
            }
        }
        let mut received = HashMap::new();
        let mut counts = HashMap::new();
        loop {
            match rx.try_recv() {
                Ok((key, value)) => {
                    received.insert(key.clone(), value);
                    let counter = counts.entry(key).or_insert(0);
                    *counter += 1;
                }
                Err(TryRecvError::Empty) => {
                    break;
                }
                Err(TryRecvError::Disconnected) => {
                    assert!(false);
                }
            }
        }
        assert_eq!(received.len(), 1000);
        assert_eq!(counts.len(), 1000);
        for i in 0..1000 {
            assert_eq!(received.get(&i), Some(&format!("test {i} 3").to_owned()));
            assert_eq!(counts.get(&i), Some(&1));
        }
    }

    #[test]
    fn recv_timeout_times_out() {
        let (_tx, mut rx) = channel::<i64, i64>();
        assert_eq!(
            rx.recv_timeout(Duration::from_millis(100)),
            Err(RecvTimeoutError::Timeout)
        );
    }

    #[test]
    fn sender_immediately_disconnects() {
        let (tx, mut rx) = channel::<i64, i64>();
        assert_eq!(
            rx.try_recv(),
            Err(TryRecvError::Empty)
        );
        drop(tx);
        assert_eq!(
            rx.recv_timeout(Duration::from_millis(100)),
            Err(RecvTimeoutError::Disconnected)
        );
        assert_eq!(rx.try_recv(), Err(TryRecvError::Disconnected));
        assert_eq!(rx.recv(), Err(RecvError));
    }

    #[test]
    fn sender_clones_gradually_disconnect() {
        let (tx, mut rx) = channel::<i64, i64>();
        let tx1 = tx.clone();
        let tx2 = tx.clone();
        assert_eq!(
            rx.try_recv(),
            Err(TryRecvError::Empty)
        );
        drop(tx);
        drop(tx2);
        assert_eq!(
            rx.try_recv(),
            Err(TryRecvError::Empty)
        );
        drop(tx1);
        assert_eq!(
            rx.recv_timeout(Duration::from_millis(100)),
            Err(RecvTimeoutError::Disconnected)
        );
        assert_eq!(rx.try_recv(), Err(TryRecvError::Disconnected));
        assert_eq!(rx.recv(), Err(RecvError));
    }

    #[test]
    fn receiver_immediately_disconnects() {
        let (mut tx, rx) = channel();
        drop(rx);
        assert_eq!(tx.send(1, 2), Err(SendError((1, 2))));
    }
}
