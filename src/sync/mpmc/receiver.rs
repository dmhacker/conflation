use parking_lot::{Condvar, Mutex};
use std::hash::Hash;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{RecvError, RecvTimeoutError, TryRecvError};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::control::ControlBlock;
use super::signal::{Signaller, SignallerResult};

pub struct Receiver<K, V> {
    pub(super) refcount: Arc<AtomicUsize>,
    pub(super) control: Arc<Mutex<ControlBlock<K, V>>>,
}

impl<K, V> Drop for Receiver<K, V> {
    fn drop(&mut self) {
        let remaining = self.refcount.fetch_sub(1, Ordering::SeqCst) - 1;
        if remaining == 0 {
            self.control.lock().disconnected = true;
        }
    }
}

impl<K, V> Clone for Receiver<K, V> {
    fn clone(&self) -> Receiver<K, V> {
        self.refcount.fetch_add(1, Ordering::SeqCst);
        Receiver {
            refcount: self.refcount.clone(),
            control: self.control.clone(),
        }
    }
}

pub struct Iter<'a, K: 'a, V: 'a> {
    receiver: &'a Receiver<K, V>,
}

impl<'a, K, V> Iterator for Iter<'a, K, V>
where
    K: Eq,
    K: Hash,
{
    type Item = (K, V);

    fn next(&mut self) -> Option<(K, V)> {
        match self.receiver.recv() {
            Ok(head) => Some(head),
            Err(_) => None,
        }
    }
}

pub struct TryIter<'a, K: 'a, V: 'a> {
    receiver: &'a Receiver<K, V>,
}

impl<'a, K, V> Iterator for TryIter<'a, K, V>
where
    K: Eq,
    K: Hash,
{
    type Item = (K, V);

    fn next(&mut self) -> Option<(K, V)> {
        match self.receiver.try_recv() {
            Ok(head) => Some(head),
            Err(_) => None,
        }
    }
}

const DEFAULT_TOKEN: i64 = 0;

impl<K, V> Receiver<K, V>
where
    K: Eq,
    K: Hash,
{
    pub fn try_recv(&self) -> Result<(K, V), TryRecvError> {
        let mut control_guard = self.control.lock();
        if control_guard.unreserved_count == 0 {
            return if control_guard.disconnected {
                Err(TryRecvError::Disconnected)
            } else {
                Err(TryRecvError::Empty)
            };
        }
        match control_guard.queue.pop_front() {
            Some(head) => {
                control_guard.unreserved_count -= 1;
                Ok(head)
            }
            None => {
                if control_guard.disconnected {
                    Err(TryRecvError::Disconnected)
                } else {
                    panic!("queue was empty but reported unreserved entries");
                }
            }
        }
    }

    pub fn recv(&self) -> Result<(K, V), RecvError> {
        match self.try_install_signaller() {
            SignallerResult::Data(data) => return Ok(data),
            SignallerResult::Blocked(signaller) => {
                signaller.wait(|tokens| {
                    if tokens.len() == 0 || tokens[0] != DEFAULT_TOKEN {
                        panic!("signaller corrupted: expected token from sender")
                    }
                });
            }
            SignallerResult::Disconnected => return Err(RecvError),
        }
        self.try_recv_reserved()
    }

    pub fn recv_deadline(&self, deadline: Instant) -> Result<(K, V), RecvTimeoutError> {
        match self.try_install_signaller() {
            SignallerResult::Data(data) => return Ok(data),
            SignallerResult::Blocked(signaller) => {
                if signaller.wait_deadline(deadline, |timeout_result, tokens| {
                    if timeout_result.timed_out() {
                        return true;
                    }
                    if tokens.len() == 0 || tokens[0] != DEFAULT_TOKEN {
                        panic!("signaller corrupted: expected token from sender")
                    }
                    false
                }) {
                    return Err(RecvTimeoutError::Timeout);
                }
            }
            SignallerResult::Disconnected => return Err(RecvTimeoutError::Disconnected),
        }
        match self.try_recv_reserved() {
            Ok(data) => return Ok(data),
            Err(RecvError) => return Err(RecvTimeoutError::Disconnected),
        }
    }

    pub fn recv_timeout(&self, timeout: Duration) -> Result<(K, V), RecvTimeoutError> {
        self.recv_deadline(Instant::now() + timeout)
    }

    pub fn iter(&self) -> Iter<'_, K, V> {
        Iter { receiver: self }
    }

    pub fn try_iter(&self) -> TryIter<'_, K, V> {
        TryIter { receiver: self }
    }

    pub fn len(&self) -> usize {
        self.control.lock().queue.len()
    }

    pub fn is_disconnected(&self) -> bool {
        self.control.lock().disconnected
    }

    fn try_install_signaller(&self) -> SignallerResult<K, V> {
        let waiter = Arc::new((Mutex::new(Vec::with_capacity(1)), Condvar::new()));
        let signaller = Signaller {
            waiter: waiter.clone(),
        };
        let mut control_guard = self.control.lock();
        if control_guard.unreserved_count > 0 {
            if let Some(head) = control_guard.queue.pop_front() {
                control_guard.unreserved_count -= 1;
                return SignallerResult::Data(head);
            }
            panic!("queue was empty but reported unreserved entries");
        } else {
            if control_guard.disconnected {
                return SignallerResult::Disconnected;
            }
            control_guard
                .consumers
                .push_back((DEFAULT_TOKEN, Signaller { waiter: waiter }));
            SignallerResult::Blocked(signaller)
        }
    }

    fn try_recv_reserved(&self) -> Result<(K, V), RecvError> {
        let mut control_guard = self.control.lock();
        if let Some(head) = control_guard.queue.pop_front() {
            return Ok(head);
        }
        if control_guard.disconnected {
            return Err(RecvError);
        }
        panic!("receiver woke up to empty queue");
    }
}
