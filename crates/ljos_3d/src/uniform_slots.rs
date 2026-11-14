//! Where each model's uniforms live in a target's buffer, kept from one
//! frame to the next.
//!
//! The draw order is not the place to key them by: opaque models are drawn
//! nearest first, so a camera that moves reorders them, and slots taken in
//! drawing order would hand every model another's slot and have the whole
//! buffer written again for a scene where nothing but the camera moved. A
//! model keeps its slot for as long as it is in the scene; a model that
//! comes in takes the lowest slot nobody held in the frame before, so the
//! slots of those still there are never given away under them.

use std::collections::HashMap;
use std::hash::Hash;

pub(crate) struct UniformSlots<K> {
    /// Last frame's, each kept for the model while it is still there.
    held: HashMap<K, u32>,
    /// This frame's so far.
    placed: HashMap<K, u32>,
    /// Which slots this frame may not give a newcomer: held last frame or
    /// placed in this one.
    taken: Vec<bool>,
    /// No free slot is below it.
    free_from: usize,
}

impl<K: Copy + Eq + Hash> Default for UniformSlots<K> {
    fn default() -> Self {
        UniformSlots {
            held: HashMap::new(),
            placed: HashMap::new(),
            taken: Vec::new(),
            free_from: 0,
        }
    }
}

impl<K: Copy + Eq + Hash> UniformSlots<K> {
    /// Starts a frame: the slots of the last one stay with their models.
    pub(crate) fn begin(&mut self) {
        // Swapped rather than taken, so that both keep their room.
        std::mem::swap(&mut self.held, &mut self.placed);
        self.placed.clear();
        self.taken.clear();
        for &slot in self.held.values() {
            let slot = slot as usize;
            if slot >= self.taken.len() {
                self.taken.resize(slot + 1, false);
            }
            self.taken[slot] = true;
        }
        self.free_from = 0;
    }

    /// The slot of the model `key` this frame: the one it had, or the
    /// lowest free one. A key asked for twice in a frame has one slot.
    pub(crate) fn slot(&mut self, key: K) -> u32 {
        if let Some(&slot) = self.placed.get(&key) {
            return slot;
        }
        let slot = match self.held.get(&key) {
            Some(&slot) => slot,
            None => {
                while self.taken.get(self.free_from) == Some(&true) {
                    self.free_from += 1;
                }
                if self.free_from == self.taken.len() {
                    self.taken.push(false);
                }
                self.taken[self.free_from] = true;
                self.free_from as u32
            }
        };
        self.placed.insert(key, slot);
        slot
    }

    /// One past the highest slot this frame or the last holds: how many
    /// the buffer is to have.
    pub(crate) fn len(&self) -> usize {
        self.taken.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(slots: &mut UniformSlots<u32>, keys: &[u32]) -> Vec<u32> {
        slots.begin();
        keys.iter().map(|&k| slots.slot(k)).collect()
    }

    #[test]
    fn a_model_keeps_its_slot_whatever_order_it_is_asked_in() {
        let mut slots = UniformSlots::default();
        assert_eq!(frame(&mut slots, &[10, 20, 30]), [0, 1, 2]);
        assert_eq!(frame(&mut slots, &[30, 20, 10]), [2, 1, 0]);
        assert_eq!(frame(&mut slots, &[20, 10, 30]), [1, 0, 2]);
        assert_eq!(slots.len(), 3);
    }

    #[test]
    fn a_newcomer_takes_no_slot_a_model_still_there_holds() {
        let mut slots = UniformSlots::default();
        assert_eq!(frame(&mut slots, &[10, 20, 30]), [0, 1, 2]);
        // 40 comes first, before 10 and 20 are asked for: it gets none of
        // theirs, nor the slot 30 held in the frame before.
        assert_eq!(frame(&mut slots, &[40, 20, 10]), [3, 1, 0]);
        // 30's slot was left a frame ago, so it is free again.
        assert_eq!(frame(&mut slots, &[50, 40, 20, 10]), [2, 3, 1, 0]);
        assert_eq!(slots.len(), 4);
    }

    #[test]
    fn a_key_asked_for_twice_has_one_slot() {
        let mut slots = UniformSlots::default();
        assert_eq!(frame(&mut slots, &[10, 10, 20]), [0, 0, 1]);
    }
}
