//! A cache that keeps what the last two frames used and nothing else.
//!
//! Laid-out text is the biggest thing a view keeps (about 25 KB per
//! paragraph), so a cache that only empties when it gets "too big" grows
//! with everything ever shown. This one follows the screen instead: each
//! [`FrameCache::next_frame`] drops what wasn't used since the one before.
//!
//! Two frames, not one: iced holds only weak references to paragraphs until
//! it renders, so what the previous frame drew must outlive the next draw's
//! start.

use std::collections::HashMap;
use std::mem;

pub struct FrameCache<V> {
    current: HashMap<u64, V>,
    previous: HashMap<u64, V>,
}

impl<V> Default for FrameCache<V> {
    fn default() -> Self {
        Self { current: HashMap::new(), previous: HashMap::new() }
    }
}

impl<V: Clone> FrameCache<V> {
    /// Call once at the start of each draw.
    pub fn next_frame(&mut self) {
        self.previous = mem::take(&mut self.current);
    }

    /// The value for `key`, made by `make` if neither this frame nor the
    /// last one had it.
    pub fn get_or_insert_with(&mut self, key: u64, make: impl FnOnce() -> V) -> V {
        if let Some(v) = self.current.get(&key) {
            return v.clone();
        }
        let v = self.previous.remove(&key).unwrap_or_else(make);
        self.current.insert(key, v.clone());
        v
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.current.len() + self.previous.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_two_frames() {
        let mut cache = FrameCache::default();
        let mut made = 0;
        let mut get = |cache: &mut FrameCache<u64>, key| {
            cache.get_or_insert_with(key, || {
                made += 1;
                key
            })
        };
        cache.next_frame();
        get(&mut cache, 1);
        get(&mut cache, 2);
        cache.next_frame();
        // Used again: kept, not made again.
        get(&mut cache, 1);
        cache.next_frame();
        get(&mut cache, 1);
        drop(get);
        assert_eq!(made, 2);
        // 2 went unused for a whole frame.
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn size_follows_the_screen() {
        let mut cache = FrameCache::default();
        for frame in 0..100u64 {
            cache.next_frame();
            for row in 0..50 {
                cache.get_or_insert_with(frame * 50 + row, || row);
            }
        }
        assert_eq!(cache.len(), 100);
    }
}
