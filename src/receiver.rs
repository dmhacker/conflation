use parking_lot::{Condvar, Mutex};
use pin_project::{pin_project, pinned_drop};
use std::future::Future;
use std::hash::Hash;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{RecvError, RecvTimeoutError, TryRecvError};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use super::control::ControlBlock;
use super::signal::{AnySignaller, AsyncSignaller, Signaller, SignallerResult, SyncSignaller};

/// The receiving end of the channel.
/// 
/// Values obtained through the receiver are only accessible
/// via one consumer. They are not broadcasted to multiple
/// consumers.
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

enum RecvFutureState {
    Waiting(Arc<AtomicBool>),
    Completed,
}

#[pin_project(PinnedDrop)]
pub struct RecvFuture<'a, K, V> {
    receiver: &'a Receiver<K, V>,
    state: RecvFutureState,
}

#[pinned_drop]
impl<'a, K, V> PinnedDrop for RecvFuture<'a, K, V> {
    fn drop(self: Pin<&mut Self>) {
        let this = self.project();
        if let RecvFutureState::Waiting(woken) = this.state {
            let mut control_guard = this.receiver.control.lock();
            if woken.load(Ordering::Relaxed) {
                if let Some(signaller) = control_guard.consumers.pop_back() {
                    signaller.signal();
                }
            } else {
                control_guard.consumers.retain(|signaller| match signaller {
                    AnySignaller::Async(async_signaller) => {
                        !Arc::ptr_eq(&woken, &async_signaller.woken)
                    }
                    _ => true,
                });
            }
        }
    }
}

impl<'a, K, V> Future for RecvFuture<'a, K, V>
where
    K: Hash,
    K: Eq,
{
    type Output = Result<(K, V), RecvError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        if let RecvFutureState::Completed = this.state {
            panic!("completed future was polled")
        }
        let mut control_guard = this.receiver.control.lock();
        match (control_guard.queue.pop_front(), control_guard.disconnected) {
            (Some(data), _) => {
                *this.state = RecvFutureState::Completed;
                Poll::Ready(Ok(data))
            }
            (None, true) => {
                *this.state = RecvFutureState::Completed;
                Poll::Ready(Err(RecvError))
            }
            (None, false) => {
                if let RecvFutureState::Waiting(woken) = this.state {
                    woken.store(false, Ordering::Relaxed);
                    control_guard
                        .consumers
                        .push_back(AnySignaller::Async(AsyncSignaller {
                            waker: cx.waker().clone(),
                            woken: woken.clone(),
                        }));
                    Poll::Pending
                } else {
                    panic!("expected future to be in waiting state")
                }
            }
        }
    }
}

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
        let waiter = Arc::new((Mutex::new(false), Condvar::new()));
        let mut control_guard = self.control.lock();
        match (control_guard.queue.pop_front(), control_guard.disconnected) {
            (Some(data), _) => SignallerResult::Data(data),
            (None, true) => SignallerResult::Disconnected,
            (None, false) => {
                control_guard
                    .consumers
                    .push_back(AnySignaller::Sync(SyncSignaller {
                        waiter: waiter.clone(),
                    }));
                SignallerResult::Blocked(SyncSignaller { waiter: waiter })
            }
        }
    }

    pub fn recv(&self) -> Result<(K, V), RecvError> {
        loop {
            match self.try_install_signaller() {
                SignallerResult::Data(data) => return Ok(data),
                SignallerResult::Blocked(signaller) => {
                    signaller.wait();
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
                    if signaller.wait_deadline(deadline).timed_out() {
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

    pub fn recv_async(&self) -> RecvFuture<'_, K, V> {
        RecvFuture {
            receiver: self,
            state: RecvFutureState::Waiting(Arc::new(AtomicBool::new(false))),
        }
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
