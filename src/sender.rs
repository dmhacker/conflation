use parking_lot::Mutex;
use std::hash::Hash;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::SendError;
use std::sync::Arc;

use super::control::ControlBlock;
use super::signal::Signaller;

/// The transmitting end of the channel.
///
/// Values submitted through the sender will be received by only one
/// consumer, except in the event of conflation, in which case the
/// message is dropped & no consumer receives it.
pub struct Sender<'a, K, V> {
    pub(super) refcount: Arc<AtomicUsize>,
    pub(super) control: Arc<Mutex<ControlBlock<'a, K, V>>>,
}

impl<K, V> Drop for Sender<'_, K, V> {
    fn drop(&mut self) {
        let remaining = self.refcount.fetch_sub(1, Ordering::Relaxed) - 1;
        if remaining == 0 {
            let mut control_guard = self.control.lock();
            // Receivers could have already disconnected
            if !control_guard.disconnected {
                control_guard.disconnected = true;
                // If there are any waiting consumers that will be
                // woken by a disconnect, there should be no items
                // currently in the queue. Upon disconnect, no more items
                // can be added to the queue, so all of these consumers
                // will receive a disconnect upon polling.
                for signaller in control_guard.consumers.drain(..) {
                    signaller.signal();
                }
            }
        }
    }
}

impl<'a, K, V> Clone for Sender<'a, K, V> {
    fn clone(&self) -> Sender<'a, K, V> {
        self.refcount.fetch_add(1, Ordering::Relaxed);
        Sender {
            refcount: self.refcount.clone(),
            control: self.control.clone(),
        }
    }
}

impl<K, V> Sender<'_, K, V>
where
    K: Eq,
    K: Hash,
{
    /// Attempts to send a message to an unbounded channel.
    ///
    /// This will fail with [`SendError`] if all receivers have been disconnected.
    ///
    /// As the channel is unbounded, this function will never block.
    pub fn send(&self, key: K, value: V) -> Result<(), SendError<(K, V)>> {
        let mut control_guard = self.control.lock();
        if control_guard.disconnected {
            return Err(SendError((key, value)));
        }
        control_guard.queue.insert(key, value);
        if let Some(signaller) = control_guard.consumers.pop_back() {
            signaller.signal();
        }
        Ok(())
    }

    /// Returns the number of messages currently queued in the channel.
    pub fn len(&self) -> usize {
        self.control.lock().queue.len()
    }

    /// Returns true if all senders or all receivers for this channel are dropped.
    pub fn is_disconnected(&self) -> bool {
        self.control.lock().disconnected
    }
}
