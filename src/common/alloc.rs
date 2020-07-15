use std::{collections::LinkedList, mem};

use slab::Slab;

/// A slab allocator with a linked list of allocations.
///
/// This allocator trades O(1) random access by key, a property of
/// [`Slab`](slab::Slab), for the ability to iterate only those entries that are
/// actually allocated. This significantly reduces the cost of `retain()`: where
/// `Slab::retain` is O(capacity) regardless of how many values are allocated,
/// [`LinkedSlab::retain`](LinkedSlab::retain) is O(n) in the number of values.
pub struct LinkedSlab<T> {
    slab: Slab<T>,
    allocated: LinkedList<usize>,
}

impl<T> LinkedSlab<T> {
    /// Construct a new, empty `LinkedSlab` with the specified capacity.
    ///
    /// The returned allocator will be able to store exactly `capacity` without
    /// reallocating. If `capacity` is 0, the slab will not allocate.
    pub fn with_capacity(capacity: usize) -> LinkedSlab<T> {
        LinkedSlab {
            slab: Slab::with_capacity(capacity),
            allocated: LinkedList::new(),
        }
    }

    /// Return the number of values the allocator can store without reallocating.
    pub fn capacity(&self) -> usize {
        self.slab.capacity()
    }

    /// Clear the allocator of all values.
    pub fn clear(&mut self) {
        self.allocated.clear();
        self.slab.clear();
    }

    /// Return the number of stored values.
    pub fn len(&self) -> usize {
        self.slab.len()
    }

    /// Return `true` if there are no values allocated.
    pub fn is_empty(&self) -> bool {
        self.slab.is_empty()
    }

    /// Return an iterator over the allocated values.
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.allocated
            .iter()
            .map(move |key| self.slab.get(*key).unwrap())
    }

    /// Return a reference to the value associated with the given key.
    ///
    /// If the given key is not associated with a value, then None is returned.
    pub fn get(&self, key: usize) -> Option<&T> {
        self.slab.get(key)
    }

    /// Return a mutable reference to the value associated with the given key.
    ///
    /// If the given key is not associated with a value, then None is returned.
    pub fn get_mut(&mut self, key: usize) -> Option<&mut T> {
        self.slab.get_mut(key)
    }

    /// Allocate a value, returning the key assigned to the value.
    ///
    /// This operation is O(1).
    pub fn insert(&mut self, val: T) -> usize {
        let key = self.slab.insert(val);
        self.allocated.push_front(key);
        key
    }

    /// Remove and return the value associated with the given key.
    ///
    /// The key is then released and may be associated with future stored values.
    ///
    /// Note that this operation is O(n) in the number of allocated values.
    pub fn remove(&mut self, key: usize) -> T {
        self.allocated.drain_filter(|k| *k == key);
        self.slab.remove(key)
    }

    /// Return `true` if a value is associated with the given key.
    pub fn contains(&self, key: usize) -> bool {
        self.slab.contains(key)
    }

    /// Retain only the elements specified by the predicate.
    ///
    /// The predicate is permitted to modify allocated values in-place.
    ///
    /// This operation is O(n) in the number of allocated values.
    pub fn retain<F>(&mut self, mut f: F)
    where
        F: FnMut(usize, &mut T) -> bool,
    {
        // move contents out to avoid double mutable borrow of self.
        // neither LinkedList::new() nor Slab::new() allocates any memory, so
        // this is free.
        let mut allocated = mem::replace(&mut self.allocated, LinkedList::new());
        let mut slab = mem::replace(&mut self.slab, Slab::new());

        allocated.drain_filter(|k| {
            let retain = match slab.get_mut(*k) {
                Some(ref mut v) => f(*k, v),
                None => true,
            };

            if !retain {
                slab.remove(*k);
            }

            !retain
        });

        // put them back
        self.slab = slab;
        self.allocated = allocated;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::{collections::HashSet, iter::FromIterator as _};

    #[test]
    fn test_iter() {
        let values: Vec<i32> = vec![1, 3, 5, 7, 11, 13, 17, 19];

        let mut linked_slab = LinkedSlab::with_capacity(values.len());
        let mut expected = HashSet::new();

        for value in values.iter() {
            linked_slab.insert(*value);
            expected.insert(*value);
        }

        let mut actual = HashSet::new();
        for value in linked_slab.iter() {
            actual.insert(*value);
        }

        assert_eq!(expected, actual);
    }

    #[test]
    fn test_retain() {
        let mut values: Vec<i32> = vec![0, 9, 1, 8, 2, 7, 3, 6, 4, 5];

        let mut linked_slab = LinkedSlab::with_capacity(values.len());

        for value in values.iter() {
            linked_slab.insert(*value);
        }

        values.retain(|v| v % 2 == 0);
        let mut expected: HashSet<i32> = HashSet::from_iter(values.into_iter());

        linked_slab.retain(|_, v| *v % 2 == 0);

        let mut actual = HashSet::from_iter(linked_slab.iter().map(|v| *v));

        assert_eq!(expected, actual);
    }
}
