use linked_hash_map::LinkedHashMap;
use std::collections::{HashMap, VecDeque};
use std::hash::Hash;

pub trait ConflatingQueue<K, V>: Send {
    fn len(&self) -> usize;
    fn pop_front(&mut self) -> Option<(K, V)>;
    fn insert(&mut self, key: K, value: V);
}

pub struct CompactingQueue<K, V> {
    queue: VecDeque<(K, V, bool)>,
    tails: HashMap<K, usize>,
    max_compaction_ratio: f32,
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
    pub fn new() -> CompactingQueue<K, V> {
        CompactingQueue {
            queue: VecDeque::new(),
            tails: HashMap::new(),
            max_compaction_ratio: 0.9,
            min_compaction_size: 1000,
            duplicates: 0,
            shifts: 0,
        }
    }

    fn compact(&mut self) {
        if self.queue.len() <= self.min_compaction_size
            || (self.duplicates as f32) < self.max_compaction_ratio * (self.queue.len() as f32)
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
