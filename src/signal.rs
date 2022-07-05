use parking_lot::{Condvar, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::task::Waker;
use std::time::Instant;

pub(super) struct SignallerTimedOut(bool);

impl SignallerTimedOut {
    pub(super) fn timed_out(&self) -> bool {
        self.0
    }
}

pub(super) trait Signaller {
    fn signal(&self);
}

pub(super) struct SyncSignaller {
    pub(super) waiter: Arc<(Mutex<bool>, Condvar)>,
}

impl Signaller for SyncSignaller {
    fn signal(&self) {
        let (lock, cvar) = &*self.waiter;
        let mut guard = lock.lock();
        *guard = true;
        cvar.notify_one();
    }
}

impl SyncSignaller {
    pub(super) fn wait(&self) {
        let (lock, cvar) = &*self.waiter;
        let mut guard = lock.lock();
        cvar.wait_while(&mut guard, |value| !*value);
    }

    pub(super) fn wait_deadline(&self, deadline: Instant) -> SignallerTimedOut {
        let (lock, cvar) = &*self.waiter;
        let mut guard = lock.lock();
        let timeout_result = cvar.wait_while_until(&mut guard, |value| !*value, deadline);
        SignallerTimedOut(timeout_result.timed_out())
    }
}

pub(super) struct AsyncSignaller {
    pub(super) waker: Waker,
    pub(super) woken: Arc<AtomicBool>,
}

impl Signaller for AsyncSignaller {
    fn signal(&self) {
        self.waker.wake_by_ref();
        self.woken.store(true, Ordering::Relaxed);
    }
}

pub(super) enum AnySignaller {
    Async(AsyncSignaller),
    Sync(SyncSignaller),
}

impl Signaller for AnySignaller {
    fn signal(&self) {
        match self {
            AnySignaller::Async(signaller) => signaller.signal(),
            AnySignaller::Sync(signaller) => signaller.signal(),
        }
    }
}

pub(super) enum SignallerResult<K, V, S: Signaller> {
    Data((K, V)),
    Blocked(S),
    Disconnected,
}
