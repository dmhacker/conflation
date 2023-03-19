use linked_hash_map::LinkedHashMap;
use std::hash::Hash;

pub struct ConflatingQueue<K, V> {
    queue: LinkedHashMap<K, V>,
}

impl<K, V> ConflatingQueue<K, V>
where
    K: Eq,
    K: Hash,
{
    pub fn new() -> ConflatingQueue<K, V> {
        ConflatingQueue { 
            queue: LinkedHashMap::new()
        }
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn pop_front(&mut self) -> Option<(K, V)> {
        self.queue.pop_front()
    }

    pub fn insert(&mut self, key: K, value: V) {
        self.queue.insert(key, value);
    }
}
