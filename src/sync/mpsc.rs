use linked_hash_map::LinkedHashMap;
use std::hash::Hash;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{RecvError, RecvTimeoutError, SendError, TryRecvError};
use std::sync::{Arc, Condvar, Mutex, WaitTimeoutResult};
use std::time::{Duration, Instant};

struct Signaller {
    waiter: Arc<(Mutex<Vec<i64>>, Condvar)>,
}

impl Signaller {
    pub fn signal(&self, token: i64) {
        let (lock, cvar) = &*self.waiter;
        let mut tokens_guard = lock.lock().unwrap();
        tokens_guard.push(token);
        cvar.notify_one();
    }

    pub fn wait<F, T>(&self, mut f: F) -> T
    where
        F: FnMut(&Vec<i64>) -> T,
    {
        let (lock, cvar) = &*self.waiter;
        let mut tokens_guard = lock.lock().unwrap();
        tokens_guard = cvar
            .wait_while(tokens_guard, |tokens| tokens.is_empty())
            .unwrap();
        f(&*tokens_guard)
    }

    pub fn wait_deadline<F, T>(&self, deadline: Instant, mut f: F) -> T
    where
        F: FnMut(WaitTimeoutResult, &Vec<i64>) -> T,
    {
        let timeout = if deadline > Instant::now() {
            deadline - Instant::now()
        } else {
            Duration::from_millis(0)
        };
        let (lock, cvar) = &*self.waiter;
        let tokens_guard = lock.lock().unwrap();
        let timeout_result = cvar
            .wait_timeout_while(tokens_guard, timeout, |tokens| tokens.is_empty())
            .unwrap();
        f(timeout_result.1, &*timeout_result.0)
    }
}

enum SignallerResult<K, V> {
    Data((K, V)),
    Blocked(Signaller),
    Disconnected,
}

struct ControlBlock<K, V> {
    queue: LinkedHashMap<K, V>,
    disconnected: bool,
    signaller: Option<(i64, Signaller)>,
}

pub struct Sender<K, V> {
    refcount: Arc<AtomicUsize>,
    control: Arc<Mutex<ControlBlock<K, V>>>,
}

impl<K, V> Drop for Sender<K, V> {
    fn drop(&mut self) {
        let remaining = self.refcount.fetch_sub(1, Ordering::SeqCst) - 1;
        if remaining == 0 {
            let mut control_guard = self.control.lock().unwrap();
            // Receiver could have already disconnected
            if !control_guard.disconnected {
                control_guard.disconnected = true;
                if let Some((token, signaller)) = &control_guard.signaller {
                    signaller.signal(*token);
                    control_guard.signaller = None;
                }
            }
        }
    }
}

impl<K, V> Clone for Sender<K, V> {
    fn clone(&self) -> Sender<K, V> {
        self.refcount.fetch_add(1, Ordering::SeqCst);
        Sender {
            refcount: self.refcount.clone(),
            control: self.control.clone(),
        }
    }
}

impl<K, V> Sender<K, V>
where
    K: Eq,
    K: Hash,
{
    pub fn send(&self, key: K, value: V) -> Result<(), SendError<(K, V)>> {
        let mut control_guard = self.control.lock().unwrap();
        if control_guard.disconnected {
            return Err(SendError((key, value)));
        }
        control_guard.queue.insert(key, value);
        if let Some((token, signaller)) = &control_guard.signaller {
            signaller.signal(*token);
            control_guard.signaller = None;
        }
        Ok(())
    }
}

pub struct Receiver<K, V> {
    control: Arc<Mutex<ControlBlock<K, V>>>,
}

impl<K, V> Drop for Receiver<K, V> {
    fn drop(&mut self) {
        self.control.lock().unwrap().disconnected = true;
    }
}

const DEFAULT_TOKEN: i64 = 0;

impl<K, V> Receiver<K, V>
where
    K: Eq,
    K: Hash,
{
    pub fn try_recv(&self) -> Result<(K, V), TryRecvError> {
        let mut control_guard = self.control.lock().unwrap();
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

    pub fn recv(&self) -> Result<(K, V), RecvError> {
        match self.try_recv_or_install_signaller() {
            SignallerResult::Data(data) => return Ok(data),
            SignallerResult::Blocked(signaller) => {
                signaller.wait(|tokens| {
                    if tokens.len() != 0 || tokens[0] != DEFAULT_TOKEN {
                        panic!("signaller corrupted: expected token from sender")
                    }
                });
            }
            SignallerResult::Disconnected => return Err(RecvError),
        }
        match self.try_recv() {
            Ok(data) => return Ok(data),
            Err(TryRecvError::Disconnected) => return Err(RecvError),
            Err(TryRecvError::Empty) => panic!("receiver woke up to empty queue"),
        }
    }

    pub fn recv_deadline(&self, deadline: Instant) -> Result<(K, V), RecvTimeoutError> {
        match self.try_recv_or_install_signaller() {
            SignallerResult::Data(data) => return Ok(data),
            SignallerResult::Blocked(signaller) => {
                if signaller.wait_deadline(deadline, |timeout_result, tokens| {
                    if timeout_result.timed_out() {
                        return true;
                    }
                    if tokens.len() != 0 || tokens[0] != DEFAULT_TOKEN {
                        panic!("signaller corrupted: expected token from sender")
                    }
                    false
                }) {
                    return Err(RecvTimeoutError::Timeout);
                }
            }
            SignallerResult::Disconnected => return Err(RecvTimeoutError::Disconnected),
        }
        match self.try_recv() {
            Ok(data) => return Ok(data),
            Err(TryRecvError::Disconnected) => return Err(RecvTimeoutError::Timeout),
            Err(TryRecvError::Empty) => panic!("receiver woke up to empty queue"),
        }
    }

    pub fn recv_timeout(&self, timeout: Duration) -> Result<(K, V), RecvTimeoutError> {
        self.recv_deadline(Instant::now() + timeout)
    }

    fn try_recv_or_install_signaller(&self) -> SignallerResult<K, V> {
        let signaller = Signaller {
            waiter: Arc::new((Mutex::new(Vec::with_capacity(1)), Condvar::new())),
        };
        let mut control_guard = self.control.lock().unwrap();
        if let Some(head) = control_guard.queue.pop_front() {
            return SignallerResult::Data(head);
        }
        if control_guard.disconnected {
            return SignallerResult::Disconnected;
        }
        control_guard.signaller = Some((
            DEFAULT_TOKEN,
            Signaller {
                waiter: signaller.waiter.clone(),
            },
        ));
        SignallerResult::Blocked(signaller)
    }
}

pub fn unbounded<K, V>() -> (Sender<K, V>, Receiver<K, V>)
where
    K: Eq,
    K: Hash,
{
    let control = Arc::new(Mutex::new(ControlBlock::<K, V> {
        queue: LinkedHashMap::<K, V>::new(),
        disconnected: false,
        signaller: None,
    }));
    let tx = Sender::<K, V> {
        refcount: Arc::new(AtomicUsize::new(1)),
        control: control.clone(),
    };
    let rx = Receiver::<K, V> { control: control };
    (tx, rx)
}

#[cfg(test)]
mod tests {
    use crate::sync::mpsc::unbounded;
    use std::collections::HashMap;
    use std::sync::mpsc::{RecvError, RecvTimeoutError, SendError, TryRecvError};
    use std::time::{Duration, Instant};

    #[test]
    fn recv_has_no_conflations() {
        let (tx, rx) = unbounded();
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
        let (tx, rx) = unbounded();
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
        let (tx, rx) = unbounded();
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
        let (_tx, rx) = unbounded::<i64, i64>();
        assert_eq!(
            rx.recv_timeout(Duration::from_millis(100)),
            Err(RecvTimeoutError::Timeout)
        );
    }

    #[test]
    fn recv_deadline_times_out() {
        let (_tx, rx) = unbounded::<i64, i64>();
        // Deadline in the past sets a timeout of 0
        assert_eq!(
            rx.recv_deadline(Instant::now() - Duration::from_millis(100)),
            Err(RecvTimeoutError::Timeout)
        );
        assert_eq!(
            rx.recv_deadline(Instant::now() + Duration::from_millis(100)),
            Err(RecvTimeoutError::Timeout)
        );
    }

    #[test]
    fn sender_immediately_disconnects() {
        let (tx, rx) = unbounded::<i64, i64>();
        assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));
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
        let (tx, rx) = unbounded::<i64, i64>();
        let tx1 = tx.clone();
        let tx2 = tx.clone();
        assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));
        drop(tx);
        drop(tx2);
        assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));
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
        let (tx, rx) = unbounded();
        drop(rx);
        assert_eq!(tx.send(1, 2), Err(SendError((1, 2))));
    }
}
