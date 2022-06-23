use parking_lot::{Condvar, Mutex};
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
    fn signal(&self, token: i64);
}

pub(super) struct SyncSignaller {
    pub(super) waiter: Arc<(Mutex<Vec<i64>>, Condvar)>,
}

impl Signaller for SyncSignaller {
    fn signal(&self, token: i64) {
        let (lock, cvar) = &*self.waiter;
        let mut tokens_guard = lock.lock();
        tokens_guard.push(token);
        cvar.notify_one();
    }
}

impl SyncSignaller {
    pub(super) fn wait<F, T>(&self, mut f: F) -> T
    where
        F: FnMut(&Vec<i64>) -> T,
    {
        let (lock, cvar) = &*self.waiter;
        let mut tokens_guard = lock.lock();
        cvar.wait_while(&mut tokens_guard, |tokens| tokens.is_empty());
        f(&*tokens_guard)
    }

    pub(super) fn wait_deadline<F, T>(&self, deadline: Instant, mut f: F) -> T
    where
        F: FnMut(SignallerTimedOut, &Vec<i64>) -> T,
    {
        let (lock, cvar) = &*self.waiter;
        let mut tokens_guard = lock.lock();
        let timeout_result =
            cvar.wait_while_until(&mut tokens_guard, |tokens| tokens.is_empty(), deadline);
        f(
            SignallerTimedOut(timeout_result.timed_out()),
            &*tokens_guard,
        )
    }
}

pub(super) struct AsyncSignaller {
    pub(super) waker: Waker,
}

impl Signaller for AsyncSignaller {
    fn signal(&self, _token: i64) {
        // TODO(dhacker1): move token into shared vector between signaller & future
        self.waker.wake_by_ref();
    }
}

pub(super) enum AnySignaller {
    Async(AsyncSignaller),
    Sync(SyncSignaller),
}

impl Signaller for AnySignaller {
    fn signal(&self, token: i64) {
        match self {
            AnySignaller::Async(signaller) => signaller.signal(token),
            AnySignaller::Sync(signaller) => signaller.signal(token),
        }
    }
}

pub(super) enum SignallerResult<K, V, S: Signaller> {
    Data((K, V)),
    Blocked(S),
    Disconnected,
}
