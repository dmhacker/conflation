use std::collections::VecDeque;

use super::queue::ConflatingQueue;
use super::signal::AnySignaller;

pub(super) struct ControlBlock<K, V> {
    pub(super) queue: ConflatingQueue<K, V>,
    pub(super) disconnected: bool,
    pub(super) consumers: VecDeque<AnySignaller>,
}
