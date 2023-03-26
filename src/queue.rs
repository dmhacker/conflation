use linked_hash_map::LinkedHashMap;
use std::collections::{HashMap, VecDeque};
use std::hash::Hash;

/// Represents a keyed queue where the items are always
/// returned in insertion order. Items with duplicate
/// keys are allowed to be dropped according to some
/// user-defined policy, although the frequency and
/// behavior of this policy can vary by implementation.
pub trait ConflatingQueue<K, V>: Send {
    fn len(&self) -> usize;
    fn pop_front(&mut self) -> Option<(K, V)>;
    fn insert(&mut self, key: K, value: V);
}

// Queue that does not conflate any elements until a
// compaction event is triggered. The triggers for the event
// are specified as a combination of the minimum allowabale
// duplicate ratio and minimum size of the queue. The compaction
// event results in all duplicates being cleared from the queue
// (in a linear fashion).
pub struct CompactingQueue<K, V> {
    queue: VecDeque<(K, V, bool)>,
    tails: HashMap<K, usize>,
    min_compaction_ratio: f32,
    min_compaction_size: usize,
    duplicates: usize,
    shifts: usize,
}

impl<K, V> CompactingQueue<K, V>
where
    K: Eq,
    K: Hash,
    K: Clone,
{
    /// Returns a queue with the min compaction ratio set to 0.9
    /// and min compaction count set to 1000.
    ///
    /// This means that the queue will only trigger a compaction event
    /// when the number of items in the queue exceeds 1000 and when
    /// the duplicate-to-unique element ratio exceeds 0.9.
    pub fn new() -> CompactingQueue<K, V> {
        CompactingQueue::with_compaction_parameters(0.9, 1000)
    }

    /// Returns a new compacting queue using the specified parameters
    /// for triggering a compaction event.
    pub fn with_compaction_parameters(
        min_compaction_ratio: f32,
        min_compaction_usize: usize,
    ) -> CompactingQueue<K, V> {
        CompactingQueue {
            queue: VecDeque::new(),
            tails: HashMap::new(),
            min_compaction_ratio: min_compaction_ratio,
            min_compaction_size: min_compaction_usize,
            duplicates: 0,
            shifts: 0,
        }
    }

    /// Triggers a compaction event if compaction ratio & queue length
    /// conditions are met. The compaction event removes all duplicates
    /// currently present in the queue.
    fn compact(&mut self) {
        if self.queue.len() <= self.min_compaction_size
            || (self.duplicates as f32) < self.min_compaction_ratio * (self.queue.len() as f32)
        {
            return;
        }
        self.queue.retain(|(_, _, is_tail)| *is_tail);
        for (idx, item) in self.queue.iter().enumerate() {
            *self.tails.get_mut(&item.0).unwrap() = idx;
        }
        self.duplicates = 0;
        self.shifts = 0;
    }
}

impl<K, V> ConflatingQueue<K, V> for CompactingQueue<K, V>
where
    K: Eq,
    K: Hash,
    K: Clone,
    K: Send,
    V: Send,
{
    fn len(&self) -> usize {
        self.queue.len()
    }

    fn pop_front(&mut self) -> Option<(K, V)> {
        let element = self.queue.pop_front();
        self.shifts += 1;
        element.and_then(|(key, value, is_tail)| {
            if is_tail {
                self.tails.remove(&key);
            } else {
                self.duplicates -= 1;
            }
            Some((key, value))
        })
    }

    fn insert(&mut self, key: K, value: V) {
        self.queue.push_back((key.clone(), value, true));
        self.tails
            .entry(key)
            .and_modify(|tail_idx| {
                let last_idx = self.queue.len() - 1 + self.shifts;
                let mut prev_value = self.queue.get_mut(*tail_idx - self.shifts).unwrap();
                prev_value.2 = false;
                *tail_idx = last_idx;
                self.duplicates += 1;
            })
            .or_insert(self.queue.len() - 1 + self.shifts);
        self.compact()
    }
}

impl<K, V> ConflatingQueue<K, V> for VecDeque<(K, V)>
where
    K: Send,
    V: Send,
{
    fn len(&self) -> usize {
        self.len()
    }

    fn pop_front(&mut self) -> Option<(K, V)> {
        self.pop_front()
    }

    fn insert(&mut self, key: K, value: V) {
        self.push_back((key, value));
    }
}

impl<K, V> ConflatingQueue<K, V> for LinkedHashMap<K, V>
where
    K: Hash,
    K: Eq,
    K: Send,
    V: Send,
{
    fn len(&self) -> usize {
        self.len()
    }

    fn pop_front(&mut self) -> Option<(K, V)> {
        self.pop_front()
    }

    fn insert(&mut self, key: K, value: V) {
        self.insert(key, value);
    }
}
