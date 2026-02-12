//! Generational Arena
//!
//! A generational arena allocator for the browser's DOM tree.
//! Uses generational indices to detect use-after-free at runtime.
//! Zero external dependencies.

use std::hash::{Hash, Hasher};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GenIndex {
    pub index: u32,
    pub generation: u32,
}

impl Hash for GenIndex {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.index.hash(state);
        self.generation.hash(state);
    }
}

struct Slot<T> {
    generation: u32,
    value: Option<T>,
    next_free: Option<u32>,
}

pub struct Arena<T> {
    slots: Vec<Slot<T>>,
    free_head: Option<u32>,
    len: usize,
}

impl<T> Default for Arena<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Arena<T> {
    pub fn new() -> Self {
        Self {
            slots: Vec::new(),
            free_head: None,
            len: 0,
        }
    }

    pub fn with_capacity(cap: usize) -> Self {
        Self {
            slots: Vec::with_capacity(cap),
            free_head: None,
            len: 0,
        }
    }

    pub fn allocate(&mut self, value: T) -> GenIndex {
        if let Some(i) = self.free_head {
            let slot = &mut self.slots[i as usize];
            self.free_head = slot.next_free;
            slot.next_free = None;
            slot.value = Some(value);
            self.len += 1;
            GenIndex {
                index: i,
                generation: slot.generation,
            }
        } else {
            let i = self.slots.len() as u32;
            self.slots.push(Slot {
                generation: 0,
                value: Some(value),
                next_free: None,
            });
            self.len += 1;
            GenIndex {
                index: i,
                generation: 0,
            }
        }
    }

    pub fn get(&self, id: GenIndex) -> Option<&T> {
        let slot = self.slots.get(id.index as usize)?;
        if slot.generation != id.generation {
            return None;
        }
        slot.value.as_ref()
    }

    pub fn get_mut(&mut self, id: GenIndex) -> Option<&mut T> {
        let slot = self.slots.get_mut(id.index as usize)?;
        if slot.generation != id.generation {
            return None;
        }
        slot.value.as_mut()
    }

    pub fn deallocate(&mut self, id: GenIndex) -> Option<T> {
        let slot = self.slots.get_mut(id.index as usize)?;
        if slot.generation != id.generation {
            return None;
        }
        let value = slot.value.take()?;
        slot.generation = slot.generation.wrapping_add(1);
        slot.next_free = self.free_head;
        self.free_head = Some(id.index);
        self.len -= 1;
        Some(value)
    }

    pub fn contains(&self, id: GenIndex) -> bool {
        self.get(id).is_some()
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn clear(&mut self) {
        self.free_head = None;
        let total = self.slots.len();
        for (i, slot) in self.slots.iter_mut().enumerate() {
            if slot.value.is_some() {
                slot.value = None;
                slot.generation = slot.generation.wrapping_add(1);
            }
            slot.next_free = if i + 1 < total {
                Some((i + 1) as u32)
            } else {
                None
            };
        }
        if total > 0 {
            self.free_head = Some(0);
        }
        self.len = 0;
    }

    pub fn iter(&self) -> ArenaIter<'_, T> {
        ArenaIter {
            slots: &self.slots,
            index: 0,
        }
    }

    pub fn iter_mut(&mut self) -> ArenaIterMut<'_, T> {
        ArenaIterMut {
            slots: self.slots.iter_mut().enumerate(),
        }
    }
}

pub struct ArenaIter<'a, T> {
    slots: &'a [Slot<T>],
    index: usize,
}

impl<'a, T> Iterator for ArenaIter<'a, T> {
    type Item = (GenIndex, &'a T);

    fn next(&mut self) -> Option<Self::Item> {
        while self.index < self.slots.len() {
            let i = self.index;
            self.index += 1;
            let slot = &self.slots[i];
            if let Some(ref value) = slot.value {
                return Some((
                    GenIndex {
                        index: i as u32,
                        generation: slot.generation,
                    },
                    value,
                ));
            }
        }
        None
    }
}

pub struct ArenaIterMut<'a, T> {
    slots: std::iter::Enumerate<std::slice::IterMut<'a, Slot<T>>>,
}

impl<'a, T> Iterator for ArenaIterMut<'a, T> {
    type Item = (GenIndex, &'a mut T);

    fn next(&mut self) -> Option<Self::Item> {
        for (i, slot) in self.slots.by_ref() {
            if let Some(ref mut value) = slot.value {
                return Some((
                    GenIndex {
                        index: i as u32,
                        generation: slot.generation,
                    },
                    value,
                ));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_allocate_get() {
        let mut arena = Arena::new();
        let id = arena.allocate(42);
        assert_eq!(arena.get(id), Some(&42));
        assert_eq!(arena.len(), 1);
    }

    #[test]
    fn deallocate_returns_value() {
        let mut arena = Arena::new();
        let id = arena.allocate("hello");
        assert_eq!(arena.deallocate(id), Some("hello"));
        assert_eq!(arena.len(), 0);
        assert_eq!(arena.get(id), None);
    }

    #[test]
    fn generation_check() {
        let mut arena = Arena::new();
        let id1 = arena.allocate(10);
        arena.deallocate(id1);
        let id2 = arena.allocate(20);
        assert_eq!(id1.index, id2.index);
        assert_ne!(id1.generation, id2.generation);
        assert_eq!(arena.get(id1), None);
        assert_eq!(arena.get(id2), Some(&20));
    }

    #[test]
    fn free_list_reuse() {
        let mut arena = Arena::new();
        let a = arena.allocate(1);
        let b = arena.allocate(2);
        let c = arena.allocate(3);
        arena.deallocate(b);
        arena.deallocate(a);
        let d = arena.allocate(4);
        assert_eq!(d.index, a.index);
        let e = arena.allocate(5);
        assert_eq!(e.index, b.index);
        assert_eq!(arena.len(), 3);
    }

    #[test]
    fn iteration() {
        let mut arena = Arena::new();
        let _a = arena.allocate(10);
        let b = arena.allocate(20);
        let _c = arena.allocate(30);
        arena.deallocate(b);
        let values: Vec<i32> = arena.iter().map(|(_, v)| *v).collect();
        assert_eq!(values, vec![10, 30]);
    }

    #[test]
    fn iter_mut() {
        let mut arena = Arena::new();
        let a = arena.allocate(1);
        let _b = arena.allocate(2);
        for (_, v) in arena.iter_mut() {
            *v *= 10;
        }
        assert_eq!(arena.get(a), Some(&10));
    }

    #[test]
    fn contains() {
        let mut arena = Arena::new();
        let id = arena.allocate(42);
        assert!(arena.contains(id));
        arena.deallocate(id);
        assert!(!arena.contains(id));
    }

    #[test]
    fn clear() {
        let mut arena = Arena::new();
        let a = arena.allocate(1);
        let _b = arena.allocate(2);
        arena.clear();
        assert_eq!(arena.len(), 0);
        assert!(!arena.contains(a));
        let c = arena.allocate(3);
        assert_eq!(c.index, 0);
        assert_eq!(arena.get(c), Some(&3));
    }

    #[test]
    fn double_deallocate() {
        let mut arena = Arena::new();
        let id = arena.allocate(42);
        assert_eq!(arena.deallocate(id), Some(42));
        assert_eq!(arena.deallocate(id), None);
    }

    #[test]
    fn out_of_range() {
        let arena: Arena<i32> = Arena::new();
        let bad = GenIndex { index: 999, generation: 0 };
        assert_eq!(arena.get(bad), None);
    }

    #[test]
    fn get_mut_modify() {
        let mut arena = Arena::new();
        let id = arena.allocate(vec![1, 2, 3]);
        arena.get_mut(id).unwrap().push(4);
        assert_eq!(arena.get(id).unwrap(), &vec![1, 2, 3, 4]);
    }

    #[test]
    fn generation_wrapping() {
        let mut arena = Arena::new();
        arena.slots.push(Slot {
            generation: u32::MAX,
            value: Some(42),
            next_free: None,
        });
        arena.len = 1;
        let id = GenIndex { index: 0, generation: u32::MAX };
        arena.deallocate(id);
        let new_id = arena.allocate(99);
        assert_eq!(new_id.generation, 0);
        assert_eq!(arena.get(new_id), Some(&99));
    }

    #[test]
    fn stress_test() {
        let mut arena = Arena::new();
        let mut ids = Vec::new();
        for i in 0..100 {
            ids.push(arena.allocate(i));
        }
        assert_eq!(arena.len(), 100);
        for i in (0..100).step_by(2) {
            arena.deallocate(ids[i]);
        }
        assert_eq!(arena.len(), 50);
        for i in 0..50 {
            arena.allocate(1000 + i);
        }
        assert_eq!(arena.len(), 100);
    }
}
