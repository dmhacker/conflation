use parking_lot::{Condvar, Mutex};
use std::future::Future;
use std::hash::Hash;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{RecvError, RecvTimeoutError, TryRecvError};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use super::control::ControlBlock;
use super::signal::{AnySignaller, AsyncSignaller, SignallerResult, SyncSignaller};

pub struct Receiver<K, V> {
    pub(super) refcount: Arc<AtomicUsize>,
    pub(super) control: Arc<Mutex<ControlBlock<K, V>>>,
}

impl<K, V> Drop for Receiver<K, V> {
    fn drop(&mut self) {
        let remaining = self.refcount.fetch_sub(1, Ordering::Relaxed) - 1;
        if remaining == 0 {
            self.control.lock().disconnected = true;
        }
    }
}

impl<K, V> Clone for Receiver<K, V> {
    fn clone(&self) -> Receiver<K, V> {
        self.refcount.fetch_add(1, Ordering::Relaxed);
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

pub struct RecvFuture<'a, K, V> {
    receiver: &'a Receiver<K, V>,
}

impl<'a, K, V> Future for RecvFuture<'a, K, V>
where
    K: Hash,
    K: Eq,
{
    type Output = Result<(K, V), RecvError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut control_guard = self.receiver.control.lock();
        if let Some(head) = control_guard.queue.pop_front() {
            Poll::Ready(Ok(head))
        } else if control_guard.disconnected {
            Poll::Ready(Err(RecvError))
        } else {
            control_guard.consumers.push_back((
                DEFAULT_TOKEN,
                AnySignaller::Async(AsyncSignaller {
                    waker: cx.waker().clone(),
                }),
            ));
            Poll::Pending
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
        match (control_guard.queue.pop_front(), control_guard.disconnected) {
            (Some(data), _) => Ok(data),
            (None, true) => Err(TryRecvError::Disconnected),
            (None, false) => Err(TryRecvError::Empty),
        }
    }

    fn try_install_signaller(&self) -> SignallerResult<K, V, SyncSignaller> {
        let waiter = Arc::new((Mutex::new(Vec::with_capacity(1)), Condvar::new()));
        let mut control_guard = self.control.lock();
        if let Some(head) = control_guard.queue.pop_front() {
            return SignallerResult::Data(head);
        } else {
            if control_guard.disconnected {
                return SignallerResult::Disconnected;
            }
            control_guard.consumers.push_back((
                DEFAULT_TOKEN,
                AnySignaller::Sync(SyncSignaller {
                    waiter: waiter.clone(),
                }),
            ));
            SignallerResult::Blocked(SyncSignaller { waiter: waiter })
        }
    }

    pub fn recv(&self) -> Result<(K, V), RecvError> {
        loop {
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
        }
    }

    pub fn recv_deadline(&self, deadline: Instant) -> Result<(K, V), RecvTimeoutError> {
        loop {
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
        }
    }

    pub fn recv_timeout(&self, timeout: Duration) -> Result<(K, V), RecvTimeoutError> {
        self.recv_deadline(Instant::now() + timeout)
    }

    pub async fn recv_async(&self) -> RecvFuture<'_, K, V> {
        RecvFuture { receiver: self }
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
}
