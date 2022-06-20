use parking_lot::{Condvar, Mutex, WaitTimeoutResult};
use std::sync::Arc;
use std::time::Instant;

pub(super) struct Signaller {
    pub(super) waiter: Arc<(Mutex<Vec<i64>>, Condvar)>,
}

impl Signaller {
    pub(super) fn signal(&self, token: i64) {
        let (lock, cvar) = &*self.waiter;
        let mut tokens_guard = lock.lock();
        tokens_guard.push(token);
        cvar.notify_one();
    }

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
        F: FnMut(WaitTimeoutResult, &Vec<i64>) -> T,
    {
        let (lock, cvar) = &*self.waiter;
        let mut tokens_guard = lock.lock();
        let timeout_result =
            cvar.wait_while_until(&mut tokens_guard, |tokens| tokens.is_empty(), deadline);
        f(timeout_result, &*tokens_guard)
    }
}

pub(super) enum SignallerResult<K, V> {
    Data((K, V)),
    Blocked(Signaller),
    Disconnected,
}
