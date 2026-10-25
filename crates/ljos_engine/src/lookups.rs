//! Counts of lookups by name, for budget tests: the component plan
//! (docs/adr/0002-component-plans.md) is meant to take string lookups off
//! the per-instance path, and these counts are how a test pins that.
//!
//! Only with the `lookup-counts` feature; without it the counting calls
//! compile to nothing and [`lookups`] does not exist, so a test cannot read
//! zeros that were never counted.

/// Lookups by name since the thread started, by what was looked up.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Lookups {
    /// A property, signal or method of a type, by name.
    pub members: u64,
    /// A type in the registry, by module and name.
    pub types: u64,
    /// An id or context property in a context chain, by name.
    pub names: u64,
    /// A string interned as an [`crate::Atom`]: a hash of the text.
    pub atoms: u64,
}

impl std::ops::Sub for Lookups {
    type Output = Lookups;

    fn sub(self, o: Lookups) -> Lookups {
        Lookups {
            members: self.members - o.members,
            types: self.types - o.types,
            names: self.names - o.names,
            atoms: self.atoms - o.atoms,
        }
    }
}

#[cfg(feature = "lookup-counts")]
thread_local! {
    // Per thread, so tests running side by side do not count each other.
    static COUNTS: std::cell::Cell<Lookups> = const {
        std::cell::Cell::new(Lookups { members: 0, types: 0, names: 0, atoms: 0 })
    };
}

#[cfg(feature = "lookup-counts")]
fn bump(f: impl FnOnce(&mut Lookups)) {
    COUNTS.with(|c| {
        let mut l = c.get();
        f(&mut l);
        c.set(l);
    });
}

#[inline(always)]
pub(crate) fn member() {
    #[cfg(feature = "lookup-counts")]
    bump(|l| l.members += 1);
}

#[inline(always)]
pub(crate) fn type_() {
    #[cfg(feature = "lookup-counts")]
    bump(|l| l.types += 1);
}

#[inline(always)]
pub(crate) fn name() {
    #[cfg(feature = "lookup-counts")]
    bump(|l| l.names += 1);
}

#[inline(always)]
pub(crate) fn atom() {
    #[cfg(feature = "lookup-counts")]
    bump(|l| l.atoms += 1);
}

/// The lookups this thread has made so far.
#[cfg(feature = "lookup-counts")]
pub fn lookups() -> Lookups {
    COUNTS.with(std::cell::Cell::get)
}
