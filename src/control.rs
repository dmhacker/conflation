use std::collections::VecDeque;

use super::queue::ConflatingQueue;
use super::signal::AnySignaller;

pub(super) struct ControlBlock<'a, K, V> {
    pub(super) queue: Box<dyn ConflatingQueue<K, V> + 'a>,
    pub(super) disconnected: bool,
    pub(super) consumers: VecDeque<AnySignaller>,
}
