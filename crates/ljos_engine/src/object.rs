//! The native side of an object ([`QmlObject`]) and the engine's per-object
//! record ([`ObjectHeader`], ARCHITECTURE.md §6.1).

use std::any::{Any, TypeId};
use std::rc::Rc;

use ljos_reactive::{NodeId, Reader, ScopeId};
use rustc_hash::FxHashMap;
use smallvec::SmallVec;

use crate::atom::Atom;
use crate::engine::Engine;
use crate::error::EngineError;
use crate::ids::{ContextId, MethodIndex, ObjectId, PropIndex};
use crate::meta::{AttachedTypeId, MetaObject, MetaRef};
use crate::value::Value;

/// The typed Rust fields of an object. Implemented by
/// `#[derive(QmlObject)]`.
///
/// A native type that extends another holds it as a field (`#[qml(base)]`)
/// and returns it from [`QmlObject::base`]; the engine walks that chain to
/// find the struct that declares a property.
pub trait QmlObject: Any {
    /// The description of the Rust type (not of a QML type built on it).
    fn meta(&self) -> &'static MetaObject;
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    /// The base type's part of this object.
    fn base(&self) -> Option<&dyn QmlObject> {
        None
    }
    fn base_mut(&mut self) -> Option<&mut dyn QmlObject> {
        None
    }
    /// The part of this object's native chain that is a `ty`, or `None`
    /// to have [`find_native`] ask each part its type and its base. The
    /// derive answers itself, in one call where the walk makes two a part:
    /// every read of a native field looks for its part.
    fn part(&self, _ty: TypeId) -> Option<Option<&dyn QmlObject>> {
        None
    }
    /// Whether [`QmlObject::part_mut`] answers. Asked first: a mutable
    /// borrow handed back from one arm cannot be walked in the other.
    fn answers_parts(&self) -> bool {
        false
    }
    /// Mutable [`QmlObject::part`]; read when [`QmlObject::answers_parts`].
    fn part_mut(&mut self, _ty: TypeId) -> Option<&mut dyn QmlObject> {
        None
    }
}

/// Static facts about a native type, for generated code.
pub trait QmlType: QmlObject + Sized {
    /// Length of the flattened property table. A constant, so derived types
    /// can number their own properties at compile time.
    const PROPERTY_COUNT: u32;
    fn static_meta() -> &'static MetaObject;
}

/// The part of `root`'s native chain that is a `ty`.
pub fn find_native(root: &dyn QmlObject, ty: TypeId) -> Option<&dyn QmlObject> {
    if let Some(found) = root.part(ty) {
        return found;
    }
    let mut cur = Some(root);
    while let Some(o) = cur {
        if o.as_any().type_id() == ty {
            return Some(o);
        }
        cur = o.base();
    }
    None
}

/// Mutable [`find_native`].
pub fn find_native_mut(root: &mut dyn QmlObject, ty: TypeId) -> Option<&mut dyn QmlObject> {
    if root.answers_parts() {
        return root.part_mut(ty);
    }
    if root.as_any().type_id() == ty {
        return Some(root);
    }
    root.base_mut().and_then(|b| find_native_mut(b, ty))
}

/// The `T` in `root`'s native chain.
pub fn downcast_chain<T: QmlObject>(root: &dyn QmlObject) -> Option<&T> {
    find_native(root, TypeId::of::<T>())?
        .as_any()
        .downcast_ref()
}

/// Mutable [`downcast_chain`].
pub fn downcast_chain_mut<T: QmlObject>(root: &mut dyn QmlObject) -> Option<&mut T> {
    find_native_mut(root, TypeId::of::<T>())?
        .as_any_mut()
        .downcast_mut()
}

bitflags::bitflags! {
    /// Lifecycle and ownership state.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
    pub struct ObjectFlags: u16 {
        /// `on_complete` has run.
        const COMPLETED = 1 << 0;
        /// Owned by the JS garbage collector rather than a parent.
        const JS_OWNED = 1 << 1;
        /// `destroy()` was called; it goes at the next deferred-delete pass.
        const PENDING_DELETE = 1 << 2;
        /// Parked in a delegate pool.
        const POOLED = 1 << 3;
        /// Its scope is frozen (off-screen, §8.3).
        const FROZEN = 1 << 4;
        /// Being torn down right now.
        const DESTROYING = 1 << 5;
        /// `Component.onDestruction` has run for it and what it owns.
        const DESTRUCTION_TOLD = 1 << 6;
        /// A property of it has an interceptor (hooks.rs): a write looks
        /// for it only then.
        const INTERCEPTED = 1 << 7;
    }
}

/// Everything the engine keeps per object. Fields are crate-private; the
/// engine's API is the way in.
pub struct ObjectHeader {
    pub(crate) meta: MetaRef,
    pub(crate) native: Box<dyn QmlObject>,
    /// QML-declared properties, by slot.
    pub(crate) dynamic: Box<[Value]>,
    /// The QObject parent: destroying it destroys this.
    pub(crate) owner: Option<ObjectId>,
    pub(crate) children: Vec<ObjectId>,
    /// The Item parent; may differ from the owner.
    pub(crate) visual_parent: Option<ObjectId>,
    pub(crate) visual_children: Vec<ObjectId>,
    pub(crate) context: ContextId,
    /// Attached objects, created on first use.
    pub(crate) attached: SmallVec<[(AttachedTypeId, ObjectId); 1]>,
    /// What only some objects need (QML-declared types, aliases, scripts).
    pub(crate) ext: Option<Box<ObjectExt>>,
    pub(crate) flags: ObjectFlags,
    /// Graph scope owning this object's nodes, created with the first node.
    pub(crate) scope: Option<ScopeId>,
    /// Reactive node per property, allocated on first observation or bind.
    pub(crate) nodes: Option<Box<[Option<NodeId>]>>,
    /// Properties written while they had no node, for provenance.
    pub(crate) assigned: SmallVec<[u64; 2]>,
    /// Tracked reads of properties that have no node.
    pub(crate) field_reads: Option<Box<FieldReads>>,
}

/// Who read which property of an object while it had no node. Most
/// properties are read and never change (a delegate's colour, a text's
/// font): keeping the read here costs far less than a node made for it
/// and disposed with the object. The property gets its node, read by
/// exactly the runs that read it, when it next changes
/// ([`Engine::ensure_node`]).
///
/// One entry per run, with a bit per property it read: a run reads many
/// properties of one object one after another (a text's forty inputs),
/// and each read after the first is a bit set in the entry it is
/// already on.
pub(crate) struct FieldReads {
    /// Words of bits per run.
    words: usize,
    /// A bit per property some run read, or a run that has since taken
    /// another's entry: a quick no, not a yes.
    props: SmallVec<[u64; 2]>,
    runs: Vec<Reader>,
    /// `words` words per run, in the order of `runs`.
    bits: Vec<u64>,
    /// Where each reader's runs are: a reader keeps one or two, and an
    /// object read by many would otherwise be looked through whole at
    /// every run's first read. Empty while there are fewer than
    /// [`INDEX_RUNS`] runs, which are looked through instead: most
    /// objects are read by a handful, and a map for them costs more than
    /// the looking.
    at: FxHashMap<NodeId, SmallVec<[u32; 2]>>,
    /// Length past which runs that stopped counting are swept out.
    sweep_at: usize,
    /// The entry of the run that read last.
    last: usize,
}

/// Runs past which [`FieldReads`] indexes its runs by reader.
const INDEX_RUNS: usize = 16;

impl FieldReads {
    /// For an object with `len` properties.
    pub(crate) fn new(len: usize) -> Self {
        let words = len.div_ceil(64).max(1);
        FieldReads {
            words,
            props: smallvec::smallvec![0; words],
            // An object read at all is read by a few runs: its bindings,
            // its effects and their next runs.
            runs: Vec::with_capacity(4),
            bits: Vec::with_capacity(4 * words),
            at: FxHashMap::default(),
            sweep_at: 0,
            last: usize::MAX,
        }
    }

    pub(crate) fn has(&self, prop: PropIndex) -> bool {
        has_bit(&self.props, prop) && self.readers(prop).next().is_some()
    }

    fn row(&mut self, i: usize) -> &mut [u64] {
        &mut self.bits[i * self.words..(i + 1) * self.words]
    }

    /// Adds a read, unless the run already made it. `current` tells
    /// whether an entry still counts.
    pub(crate) fn add(&mut self, prop: PropIndex, r: Reader, current: impl Fn(Reader) -> bool) {
        if self.runs.get(self.last) == Some(&r) {
            let last = self.last;
            set_bit(self.row(last), prop);
            set_bit(&mut self.props, prop);
            return;
        }
        let at = match self.run_of(r, &current) {
            Run::Is(i) => i,
            // An effect that runs every frame (an item's geometry under an
            // animation) reads the same properties each time: its new run
            // takes the entry of a run before its last, which stopped
            // counting when the last finished, so the list does not grow
            // to be swept. The last run's entry stays: should this one not
            // finish, the effect still hears of a change, as by its edges.
            Run::Stale(i) => {
                self.runs[i] = r;
                self.row(i).fill(0);
                i
            }
            Run::None => {
                if self.runs.len() >= self.sweep_at {
                    self.keep(|e, _| current(e));
                    self.sweep_at = (self.runs.len() * 2).max(16);
                }
                let i = self.runs.len();
                self.runs.push(r);
                self.bits.resize(self.bits.len() + self.words, 0);
                if self.runs.len() == INDEX_RUNS {
                    self.index();
                } else if self.runs.len() > INDEX_RUNS {
                    self.at.entry(r.node).or_default().push(i as u32);
                }
                i
            }
        };
        self.last = at;
        set_bit(self.row(at), prop);
        // After the sweep, which counts the bits again.
        set_bit(&mut self.props, prop);
    }

    /// The entry of `r`, or else one of its reader's that stopped counting.
    fn run_of(&self, r: Reader, current: &impl Fn(Reader) -> bool) -> Run {
        let pick = |mine: &mut dyn Iterator<Item = usize>| {
            let mut stale = None;
            for i in mine {
                if self.runs[i] == r {
                    return Run::Is(i);
                }
                if stale.is_none() && !current(self.runs[i]) {
                    stale = Some(i);
                }
            }
            stale.map_or(Run::None, Run::Stale)
        };
        if self.runs.len() < INDEX_RUNS {
            pick(&mut (0..self.runs.len()).filter(|&i| self.runs[i].node == r.node))
        } else {
            let mine = self.at.get(&r.node).map_or(&[][..], |a| a.as_slice());
            pick(&mut mine.iter().map(|&i| i as usize))
        }
    }

    fn index(&mut self) {
        self.at.clear();
        if self.runs.len() < INDEX_RUNS {
            return;
        }
        for (i, r) in self.runs.iter().enumerate() {
            self.at.entry(r.node).or_default().push(i as u32);
        }
    }

    /// Keeps the runs `f` keeps, in their order, and the bits of what
    /// they read.
    fn keep(&mut self, mut f: impl FnMut(Reader, &[u64]) -> bool) {
        let w = self.words;
        let mut kept = 0;
        for i in 0..self.runs.len() {
            if f(self.runs[i], &self.bits[i * w..(i + 1) * w]) {
                self.runs[kept] = self.runs[i];
                self.bits.copy_within(i * w..(i + 1) * w, kept * w);
                kept += 1;
            }
        }
        self.runs.truncate(kept);
        self.bits.truncate(kept * w);
        self.last = usize::MAX;
        self.index();
        self.props.fill(0);
        for row in self.bits.chunks_exact(w) {
            for (p, b) in self.props.iter_mut().zip(row) {
                *p |= b;
            }
        }
    }

    /// Takes the readers of `prop` out.
    pub(crate) fn take(&mut self, prop: PropIndex) -> SmallVec<[Reader; 4]> {
        let mut out = SmallVec::new();
        if !self.has(prop) {
            return out;
        }
        let (word, bit) = (prop.0 as usize / 64, 1u64 << (prop.0 % 64));
        for i in 0..self.runs.len() {
            let w = &mut self.bits[i * self.words + word];
            if *w & bit != 0 {
                *w &= !bit;
                out.push(self.runs[i]);
            }
        }
        self.keep(|_, row| row.iter().any(|w| *w != 0));
        out
    }

    /// Every run that read a property.
    pub(crate) fn all(&self) -> impl Iterator<Item = Reader> + '_ {
        self.runs.iter().copied()
    }

    /// What the runs of `reader` read, each with the run that read it.
    pub(crate) fn readers_of(
        &self,
        reader: NodeId,
    ) -> impl Iterator<Item = (PropIndex, Reader)> + '_ {
        let w = self.words;
        let mine: SmallVec<[u32; 4]> = if self.runs.len() < INDEX_RUNS {
            (0..self.runs.len() as u32)
                .filter(|&i| self.runs[i as usize].node == reader)
                .collect()
        } else {
            self.at.get(&reader).map_or(SmallVec::new(), |a| a.iter().copied().collect())
        };
        mine.into_iter()
            .map(move |i| (i as usize, &self.runs[i as usize]))
            .flat_map(move |(i, r)| {
                self.bits[i * w..(i + 1) * w]
                    .iter()
                    .enumerate()
                    .flat_map(move |(k, &word)| {
                        (0..64)
                            .filter(move |b| word & (1 << b) != 0)
                            .map(move |b| (PropIndex((k * 64 + b) as u32), *r))
                    })
            })
    }

    /// The readers of `prop`, left in place.
    pub(crate) fn readers(&self, prop: PropIndex) -> impl Iterator<Item = Reader> + '_ {
        let w = self.words;
        self.runs
            .iter()
            .enumerate()
            .filter(move |(i, _)| has_bit(&self.bits[i * w..(i + 1) * w], prop))
            .map(|(_, r)| *r)
    }
}

enum Run {
    Is(usize),
    Stale(usize),
    None,
}

fn has_bit(words: &[u64], prop: PropIndex) -> bool {
    let (word, bit) = (prop.0 as usize / 64, prop.0 % 64);
    words.get(word).is_some_and(|w| w & (1 << bit) != 0)
}

fn set_bit(words: &mut [u64], prop: PropIndex) {
    let (word, bit) = (prop.0 as usize / 64, prop.0 % 64);
    if let Some(w) = words.get_mut(word) {
        *w |= 1 << bit;
    }
}

/// A native whose type builds on `QtObject` without a field for it (the
/// derive's implied base), with that part stored beside it. It is the
/// native to everything that looks at it: the chain walks to its
/// `QtObject` part as to a base field.
struct ImpliedBase {
    native: Box<dyn QmlObject>,
    base: crate::QtObject,
}

impl QmlObject for ImpliedBase {
    fn meta(&self) -> &'static MetaObject {
        self.native.meta()
    }
    fn as_any(&self) -> &dyn Any {
        self.native.as_any()
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self.native.as_any_mut()
    }
    fn base(&self) -> Option<&dyn QmlObject> {
        Some(&self.base)
    }
    fn base_mut(&mut self) -> Option<&mut dyn QmlObject> {
        Some(&mut self.base)
    }
}

/// `native`, with its `QtObject` part when its type has one only by the
/// derive's implied base.
fn with_implied_base(native: Box<dyn QmlObject>) -> Box<dyn QmlObject> {
    let qt_object = TypeId::of::<crate::QtObject>();
    let implied = native.base().is_none()
        && native.as_any().type_id() != qt_object
        && native
            .meta()
            .base
            .as_ref()
            .is_some_and(|b| b.native == Some(qt_object));
    if implied {
        Box::new(ImpliedBase {
            native,
            base: crate::QtObject::default(),
        })
    } else {
        native
    }
}

impl ObjectHeader {
    pub(crate) fn new(meta: MetaRef, native: Box<dyn QmlObject>, context: ContextId) -> Self {
        let native = with_implied_base(native);
        // Declared properties come after the native ones they build on:
        // from the end, the walk stops once every slot has its default,
        // and a type with none declared walks nothing.
        let slots = meta.dynamic_slots as usize;
        let mut dynamic = vec![Value::Undefined; slots].into_boxed_slice();
        let mut filled = vec![false; slots];
        let mut left = slots;
        for p in meta.properties.iter().rev() {
            if left == 0 {
                break;
            }
            if let crate::meta::PropertyAccess::Dynamic { slot } = p.access
                && !std::mem::replace(&mut filled[slot as usize], true)
            {
                dynamic[slot as usize] = p.ty.default_value();
                left -= 1;
            }
        }
        ObjectHeader {
            meta,
            native,
            dynamic,
            owner: None,
            children: Vec::new(),
            visual_parent: None,
            visual_children: Vec::new(),
            context,
            attached: SmallVec::new(),
            ext: None,
            flags: ObjectFlags::empty(),
            scope: None,
            nodes: None,
            assigned: SmallVec::new(),
            field_reads: None,
        }
    }

    pub(crate) fn node(&self, prop: PropIndex) -> Option<NodeId> {
        self.nodes.as_ref()?.get(prop.0 as usize).copied().flatten()
    }

    /// Whether a reader may follow `prop` without its node: a change has
    /// to make the node first ([`Engine::ensure_node`]) to reach it.
    pub(crate) fn read_without_node(&self, prop: PropIndex) -> bool {
        self.field_reads.as_ref().is_some_and(|r| r.has(prop))
    }

    /// Whether `prop` has a value still to be made on its next read
    /// ([`Engine::make_on_read`]).
    pub(crate) fn makes_on_read(&self, prop: PropIndex) -> bool {
        self.ext
            .as_ref()
            .is_some_and(|x| x.made_on_read.iter().any(|(p, _)| *p == prop))
    }

    /// Whether `prop` is still to be bound on its next read
    /// ([`Engine::bind_on_read`]).
    pub(crate) fn binds_on_read(&self, prop: PropIndex) -> bool {
        self.ext.as_ref().is_some_and(|x| {
            x.made_on_read
                .iter()
                .any(|(p, m)| *p == prop && matches!(m, crate::made_on_read::Maker::Bind(_)))
        })
    }

    pub(crate) fn set_node(&mut self, prop: PropIndex, n: NodeId) {
        let len = self.meta.properties.len();
        let nodes = self
            .nodes
            .get_or_insert_with(|| vec![None; len].into_boxed_slice());
        nodes[prop.0 as usize] = Some(n);
    }

    pub(crate) fn mark_assigned(&mut self, prop: PropIndex) {
        let (word, bit) = (prop.0 as usize / 64, prop.0 % 64);
        if self.assigned.len() <= word {
            self.assigned.resize(word + 1, 0);
        }
        self.assigned[word] |= 1 << bit;
    }

    pub(crate) fn clear_assigned(&mut self, prop: PropIndex) {
        let (word, bit) = (prop.0 as usize / 64, prop.0 % 64);
        if let Some(w) = self.assigned.get_mut(word) {
            *w &= !(1 << bit);
        }
    }

    pub(crate) fn is_assigned(&self, prop: PropIndex) -> bool {
        let (word, bit) = (prop.0 as usize / 64, prop.0 % 64);
        self.assigned.get(word).is_some_and(|w| w & (1 << bit) != 0)
    }

    pub(crate) fn ext_mut(&mut self) -> &mut ObjectExt {
        self.ext.get_or_insert_with(Box::default)
    }

    /// The target of an alias property.
    pub(crate) fn alias(&self, prop: PropIndex) -> Option<AliasTarget> {
        self.ext
            .as_ref()?
            .aliases
            .iter()
            .find(|(p, _)| *p == prop)
            .map(|(_, t)| t.clone())
    }
}

/// A method declared in QML: the engine calls it with `this` and the
/// arguments; the JS host runs it.
pub type ScriptMethod = Rc<dyn Fn(&Engine, ObjectId, &[Value]) -> Result<Value, EngineError>>;

/// Where an alias points: an object, one of its properties, or one member
/// of a property whose value has members (`label.font.family`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AliasTarget {
    pub(crate) object: ObjectId,
    pub(crate) prop: Option<PropIndex>,
    pub(crate) member: Option<Atom>,
}

/// A source position, for Qt-style messages.
#[derive(Clone, Debug)]
pub(crate) struct Location {
    pub(crate) url: Atom,
    pub(crate) line: u32,
    pub(crate) column: u32,
}

/// Where the binding on `prop` is written: the start of the property's
/// name, in the file that binds it (which is not always the file the
/// object is declared in).
#[derive(Clone, Debug)]
pub(crate) struct BindingLoc {
    pub(crate) prop: PropIndex,
    pub(crate) at: Location,
    /// What the binding is on when that is a part of the property:
    /// `font.pixelSize` for a binding stored on `font`.
    pub(crate) member: Option<Atom>,
}

/// Per-object state most native objects never need, boxed so the header
/// stays small.
#[derive(Default)]
pub(crate) struct ObjectExt {
    pub(crate) aliases: Vec<(PropIndex, AliasTarget)>,
    pub(crate) methods: Vec<(MethodIndex, ScriptMethod)>,
    /// The code of each of `methods` a script may call without the host
    /// in between (incubator/direct_methods.rs).
    pub(crate) direct: Vec<(
        MethodIndex,
        std::rc::Rc<crate::incubator::Instance>,
        ljos_ir::ExprId,
    )>,
    pub(crate) location: Option<Location>,
    /// Where the binding of each property was written, for binding-loop
    /// messages.
    pub(crate) binding_locs: Vec<BindingLoc>,
    /// Contexts made for this object's component instance; destroyed with
    /// it.
    pub(crate) contexts: Vec<ContextId>,
    /// Read by bindings holding a reference to this object, so destroying
    /// it (which disposes the node with its scope) re-runs them.
    pub(crate) alive: Option<NodeId>,
    /// The visual parent as a reactive source.
    pub(crate) parent_node: Option<NodeId>,
    /// Data kept alive as long as the object (the component instance it
    /// roots, script state).
    pub(crate) keep: Vec<Rc<dyn Any>>,
    /// Sees every write from QML or JS first (a model row sends it to its
    /// model); true: the write is done.
    pub(crate) write_hook: Option<WriteHook>,
    /// The model this object is to a view (`model: obj`), for types that
    /// are one.
    pub(crate) model: Option<Rc<dyn crate::model::ItemModel>>,
    /// Read by views that name this object as their model, so one that
    /// resolved it before it became a model (completion runs
    /// last-made-first) resolves it again.
    pub(crate) model_node: Option<NodeId>,
    /// Values made when the property is next read
    /// ([`Engine::make_on_read`]).
    pub(crate) made_on_read: Vec<(PropIndex, crate::made_on_read::Maker)>,
}

/// See [`ObjectExt::write_hook`].
pub(crate) type WriteHook = Rc<dyn Fn(&Engine, PropIndex, &Value) -> bool>;

#[cfg(test)]
mod tests {
    use super::*;
    use slotmap::KeyData;

    #[test]
    fn a_reader_run_every_frame_keeps_two_entries_and_the_last_finished_one() {
        let node = NodeId::from(KeyData::from_ffi(1));
        let mut reads = FieldReads::new(3);
        let (x, y) = (PropIndex(0), PropIndex(2));
        // Run `k` reads while run `k - 1` is the one that finished.
        for run in 1..1000u64 {
            let current = |r: Reader| r.run + 1 >= run;
            reads.add(x, Reader { node, run }, current);
            reads.add(y, Reader { node, run }, current);
        }
        assert_eq!(reads.runs.len(), 2);
        // Run 999 stopped part way: 998 still hears of a change.
        let runs: Vec<u64> = reads.take(x).iter().map(|r| r.run).collect();
        assert!(runs.contains(&998) && runs.contains(&999), "{runs:?}");
    }

    #[test]
    fn every_property_a_run_read_is_kept_once_for_it() {
        let node = NodeId::from(KeyData::from_ffi(1));
        let mut reads = FieldReads::new(70);
        let r = Reader { node, run: 1 };
        let props = [PropIndex(0), PropIndex(3), PropIndex(69)];
        for p in props.iter().chain(&props) {
            reads.add(*p, r, |_| true);
        }
        assert_eq!(reads.runs.len(), 1);
        for p in props {
            assert!(reads.has(p), "{p:?}");
        }
        assert!(!reads.has(PropIndex(1)));
        let mut read: Vec<_> = reads.readers_of(node).map(|(p, _)| p.0).collect();
        read.sort_unstable();
        assert_eq!(read, [0, 3, 69]);
        assert_eq!(reads.take(PropIndex(3)).as_slice(), [r]);
        assert!(!reads.has(PropIndex(3)) && reads.has(PropIndex(69)));
    }

    #[test]
    fn many_readers_of_one_property_each_keep_their_two_runs() {
        let mut reads = FieldReads::new(1);
        let x = PropIndex(0);
        let nodes: Vec<NodeId> = (1..=40)
            .map(|i| NodeId::from(KeyData::from_ffi(i)))
            .collect();
        for run in 1..50u64 {
            let current = |r: Reader| r.run + 1 >= run;
            for &node in &nodes {
                reads.add(x, Reader { node, run }, current);
            }
        }
        assert_eq!(reads.runs.len(), 80);
        // Each reader's runs are found where the index says they are.
        for &node in &nodes {
            let mut runs: Vec<u64> = reads.readers_of(node).map(|(_, r)| r.run).collect();
            runs.sort_unstable();
            assert_eq!(runs, [48, 49]);
        }
        assert_eq!(reads.take(x).len(), 80);
        assert!(reads.at.is_empty() && !reads.has(x));
    }
}
