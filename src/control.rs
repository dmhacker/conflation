use linked_hash_map::LinkedHashMap;
use std::collections::VecDeque;

use super::signal::AnySignaller;

pub(super) struct ControlBlock<K, V> {
    pub(super) queue: LinkedHashMap<K, V>,
    pub(super) disconnected: bool,
    pub(super) consumers: VecDeque<AnySignaller>,
}
