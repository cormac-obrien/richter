use std::ops::Range;

use arrayvec::{ArrayString, ArrayVec};

/// Maximum permitted length of a precache path.
const MAX_PRECACHE_PATH: usize = 64;

const MAX_PRECACHE_ENTRIES: usize = 256;

/// A list of resources to be loaded before entering the game.
///
/// This is used by the server to inform clients which resources (sounds and
/// models) they should load before joining. It also serves as the canonical
/// mapping of resource IDs for a given level.
// TODO: ideally, this is parameterized by the maximum number of entries, but
// it's not currently possible to do { MAX_PRECACHE_PATH * N } where N is a
// const generic parameter. In practice both models and sounds have a maximum
// value of 256.
#[derive(Debug)]
pub struct Precache {
    str_data: ArrayString<{ MAX_PRECACHE_PATH * MAX_PRECACHE_ENTRIES }>,
    items: ArrayVec<Range<usize>, MAX_PRECACHE_ENTRIES>,
}

impl Precache {
    /// Creates a new empty `Precache`.
    pub fn new() -> Precache {
        Precache {
            str_data: ArrayString::new(),
            items: ArrayVec::new(),
        }
    }

    /// Retrieves an item from the precache if the item exists.
    pub fn get(&self, index: usize) -> Option<&str> {
        if index > self.items.len() {
            return None;
        }

        let range = self.items[index].clone();
        Some(&self.str_data[range])
    }

    /// Returns the index of the target value if it exists.
    pub fn find<S>(&self, target: S) -> Option<usize>
    where
        S: AsRef<str>,
    {
        let (idx, _) = self
            .iter()
            .enumerate()
            .find(|&(_, item)| item == target.as_ref())?;
        Some(idx)
    }

    /// Adds an item to the precache.
    ///
    /// If the item already exists in the precache, this has no effect.
    pub fn precache<S>(&mut self, item: S)
    where
        S: AsRef<str>,
    {
        let item = item.as_ref();

        if item.len() > MAX_PRECACHE_PATH {
            panic!(
                "precache name (\"{}\") too long: max length is {}",
                item, MAX_PRECACHE_PATH
            );
        }

        if self.find(item).is_some() {
            // Already precached.
            return;
        }

        let start = self.str_data.len();
        self.str_data.push_str(item);
        let end = self.str_data.len();

        self.items.push(start..end);
    }

    /// Returns an iterator over the values in the precache.
    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.items
            .iter()
            .cloned()
            .map(move |range| &self.str_data[range])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_precache_one() {
        let mut p = Precache::new();

        p.precache("hello");
        assert_eq!(Some("hello"), p.get(0));
    }

    #[test]
    fn test_precache_several() {
        let mut p = Precache::new();

        let items = &["Quake", "is", "a", "1996", "first-person", "shooter"];

        for item in items {
            p.precache(item);
        }

        // Pick an element in the middle
        assert_eq!(Some("first-person"), p.get(4));

        // Check all the elements
        for (precached, &original) in p.iter().zip(items.iter()) {
            assert_eq!(precached, original);
        }
    }
}
