use linked_hash_map::LinkedHashMap;
use std::collections::VecDeque;

use super::signal::Signaller;

pub(super) struct ControlBlock<K, V> {
    pub(super) queue: LinkedHashMap<K, V>,
    pub(super) disconnected: bool,
    pub(super) unreserved_count: usize,
    pub(super) consumers: VecDeque<(i64, Signaller)>,
}
