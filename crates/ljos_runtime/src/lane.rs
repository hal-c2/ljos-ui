//! Lanes and logical owners (ARCHITECTURE.md 3.4, 9.1).

/// Priority classes, served in declaration order every frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Lane {
    /// Input handlers. Always get a turn, even in an over-budget frame.
    Input,
    /// Inbox processing and other work the frame cannot be correct without.
    /// Always gets a turn.
    Critical,
    /// Creation and updates for what is on screen.
    Visible,
    /// Cache-buffer work for what is about to scroll in. Round-robin by owner.
    Prefetch,
    /// Everything else that can wait. Round-robin by owner.
    Deferred,
    /// Only when budget is left over: GC, cache trims.
    Idle,
}

impl Lane {
    pub const COUNT: usize = 6;
    pub const ALL: [Lane; Lane::COUNT] = [
        Lane::Input,
        Lane::Critical,
        Lane::Visible,
        Lane::Prefetch,
        Lane::Deferred,
        Lane::Idle,
    ];

    pub const fn index(self) -> usize {
        self as usize
    }

    /// Whether a job in this lane is polled at least once per frame even when
    /// the frame budget is already spent: input latency and message
    /// back-pressure must not depend on how slow the previous work was.
    pub const fn always_runs(self) -> bool {
        matches!(self, Lane::Input | Lane::Critical)
    }

    /// Whether the lane serves owners in turn rather than strict FIFO, so one
    /// component with a thousand jobs cannot starve another with one.
    pub const fn round_robin_by_owner(self) -> bool {
        matches!(self, Lane::Prefetch | Lane::Deferred)
    }
}

/// A logical owner of work and state: a component boundary, a plugin, a
/// service. Index plus generation, so a stale id never aliases a recycled slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProcId {
    index: u32,
    generation: u32,
}

impl ProcId {
    pub const fn new(index: u32, generation: u32) -> Self {
        Self { index, generation }
    }

    pub const fn index(self) -> u32 {
        self.index
    }

    pub const fn generation(self) -> u32 {
        self.generation
    }
}

/// Hands out [`ProcId`]s and recycles slots with a bumped generation.
#[derive(Debug, Default)]
pub struct ProcIdAllocator {
    /// Current generation per slot.
    generations: Vec<u32>,
    live: Vec<bool>,
    free: Vec<u32>,
}

impl ProcIdAllocator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn alloc(&mut self) -> ProcId {
        if let Some(index) = self.free.pop() {
            self.live[index as usize] = true;
            return ProcId::new(index, self.generations[index as usize]);
        }
        let index = u32::try_from(self.generations.len()).expect("more than u32::MAX processes");
        self.generations.push(0);
        self.live.push(true);
        ProcId::new(index, 0)
    }

    /// Releases `id`. Returns false if it was stale or already freed.
    pub fn free(&mut self, id: ProcId) -> bool {
        if !self.is_live(id) {
            return false;
        }
        let i = id.index as usize;
        self.live[i] = false;
        self.generations[i] = self.generations[i].wrapping_add(1);
        self.free.push(id.index);
        true
    }

    pub fn is_live(&self, id: ProcId) -> bool {
        let i = id.index as usize;
        self.live.get(i).copied().unwrap_or(false) && self.generations[i] == id.generation
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lanes_are_ordered_by_priority() {
        assert!(Lane::Input < Lane::Critical && Lane::Critical < Lane::Visible);
        assert!(Lane::Deferred < Lane::Idle);
        for (i, l) in Lane::ALL.iter().enumerate() {
            assert_eq!(l.index(), i);
        }
    }

    #[test]
    fn stale_proc_ids_do_not_alias_recycled_slots() {
        let mut a = ProcIdAllocator::new();
        let first = a.alloc();
        let second = a.alloc();
        assert_ne!(first, second);
        assert!(a.free(first));
        assert!(!a.free(first), "double free is rejected");
        assert!(!a.is_live(first));
        let recycled = a.alloc();
        assert_eq!(recycled.index(), first.index());
        assert_ne!(recycled.generation(), first.generation());
        assert!(a.is_live(recycled) && !a.is_live(first));
        assert!(a.is_live(second));
    }
}
