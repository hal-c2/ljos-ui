//! `Atom`: the engine's string type.
//!
//! `Value::String` holds an [`Atom`], not an `Arc<str>`. An `Arc<str>` is a
//! fat pointer (16 bytes) and would push `Value` to 24 bytes; an `Atom` is a
//! thin pointer to an atomically counted header followed by the bytes
//! (`arcstr::ArcStr`), so it is 8 bytes, clones with one atomic increment and
//! is `Send + Sync` (values cross threads inside `UiMsg`).
//!
//! Interning is opt-in. Names (property, signal and type names, ids) go
//! through [`Atom::intern`], so equal names share one allocation and compare
//! by pointer. Runtime strings (text typed by the user, JSON from a service)
//! use [`Atom::new`] and are freed with their last reference: interning them
//! would grow the table for the life of the process. Equality never depends
//! on interning; two atoms with the same bytes are equal either way, the
//! pointer check is only a fast path.

use std::borrow::Borrow;
use std::cell::RefCell;
use std::collections::HashSet;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::ops::Deref;

use arcstr::ArcStr;

/// A cheap-to-clone, immutable, thread-safe string. See the module docs.
#[derive(Clone, Default)]
pub struct Atom(ArcStr);

thread_local! {
    // Per thread, so interning takes no lock. Atoms interned on different
    // threads are still equal by content.
    static INTERNER: RefCell<HashSet<ArcStr>> = RefCell::new(HashSet::new());
}

impl Atom {
    /// The empty string. Allocation-free.
    pub const EMPTY: Atom = Atom(ArcStr::new());

    /// [`crate::Font::DEFAULT_FAMILY`], made without allocating: every
    /// default font holds it. A literal per platform, as the macro wants
    /// one; a test holds it to [`ljos_runtime::Platform::default_family`].
    #[cfg(target_os = "macos")]
    pub(crate) const DEFAULT_FONT_FAMILY: Atom = Atom(arcstr::literal!(".AppleSystemUIFont"));
    #[cfg(not(target_os = "macos"))]
    pub(crate) const DEFAULT_FONT_FAMILY: Atom = Atom(arcstr::literal!("Sans Serif"));

    /// A fresh, uninterned atom. Use for runtime data.
    pub fn new(s: &str) -> Self {
        Atom(ArcStr::from(s))
    }

    /// The shared atom for `s`, allocating it the first time this thread
    /// sees it. Use for names, never for unbounded runtime data.
    pub fn intern(s: &str) -> Self {
        if s.is_empty() {
            return Atom::EMPTY;
        }
        crate::lookups::atom();
        INTERNER.with(|set| {
            let mut set = set.borrow_mut();
            if let Some(a) = set.get(s) {
                return Atom(a.clone());
            }
            let a = ArcStr::from(s);
            set.insert(a.clone());
            Atom(a)
        })
    }

    /// The string slice.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Whether both atoms are the same allocation (interned twins, clones).
    pub fn ptr_eq(a: &Atom, b: &Atom) -> bool {
        ArcStr::ptr_eq(&a.0, &b.0)
    }
}

impl Deref for Atom {
    type Target = str;
    fn deref(&self) -> &str {
        self.0.as_str()
    }
}

impl AsRef<str> for Atom {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Borrow<str> for Atom {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl PartialEq for Atom {
    fn eq(&self, other: &Self) -> bool {
        // `ArcStr` already checks the pointer before the bytes.
        self.0 == other.0
    }
}

impl Eq for Atom {}

impl PartialEq<str> for Atom {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for Atom {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl PartialOrd for Atom {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Atom {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.as_str().cmp(other.as_str())
    }
}

impl Hash for Atom {
    // Must hash like `str` so maps keyed by `Atom` can be queried with `&str`.
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_str().hash(state);
    }
}

impl fmt::Debug for Atom {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_str(), f)
    }
}

impl fmt::Display for Atom {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<&str> for Atom {
    fn from(s: &str) -> Self {
        Atom::new(s)
    }
}

impl From<String> for Atom {
    fn from(s: String) -> Self {
        Atom(ArcStr::from(s))
    }
}

impl From<&String> for Atom {
    fn from(s: &String) -> Self {
        Atom::new(s)
    }
}

impl serde::Serialize for Atom {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> serde::Deserialize<'de> for Atom {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = <std::borrow::Cow<'de, str>>::deserialize(d)?;
        Ok(Atom::new(&s))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interned_atoms_share_storage() {
        let a = Atom::intern("width");
        let b = Atom::intern("width");
        assert!(Atom::ptr_eq(&a, &b));
        let c = Atom::new("width");
        assert!(!Atom::ptr_eq(&a, &c));
        assert_eq!(a, c);
        assert_eq!(std::mem::size_of::<Atom>(), 8);
    }

    #[test]
    fn the_default_font_family_is_the_platforms() {
        assert_eq!(
            Atom::DEFAULT_FONT_FAMILY.as_str(),
            ljos_runtime::Platform::CURRENT.default_family()
        );
    }

    #[test]
    fn hashes_like_str() {
        let mut m = std::collections::HashMap::new();
        m.insert(Atom::new("x"), 1);
        assert_eq!(m.get("x"), Some(&1));
    }
}
