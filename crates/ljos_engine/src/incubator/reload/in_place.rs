//! A reload whose patch only gives objects other values, bindings,
//! handler bodies or function bodies is applied to the objects where they
//! stand, instead of making the roots again: every object goes on with
//! everything it holds (an animation half way, a delegate's scroll
//! position, what the user typed), and the cost is the edit's, not the
//! page's.
//!
//! The patch qualifies when every op is a `PropertyChanged` of a plain
//! name the new file gives as a value or an expression, a
//! `HandlerChanged`, or a `DeclarationChanged` of a function that takes
//! the same arguments; and the changed files are the old ones but for
//! those members: the same imports, pragmas, scripts, components,
//! objects and declarations, wherever they now are. Then what is in effect
//! is the edit alone. Anything else (an object added, a group's member, a property no
//! longer given, a signal) is made again by [`Engine::reload`].
//!
//! The objects keep the bindings they have. The edited members and
//! functions are installed from the new file, and so are the `Component`s,
//! so that a delegate made later is made from the new file. A binding,
//! handler or method made while tracking runs through a [`CodeSlot`],
//! which is pointed at the new file: an edited body runs on the next
//! signal, and what fails is told at the line it is now on. A value made a
//! binding moves the expressions after it, so an old expression is found
//! in the new file by its place among the object's; and the objects and
//! bindings are told where they now are written.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use ljos_ir::{
    CompIdx, CompiledFile, ComponentDef, ExprId, Literal, Members, ObjIdx, ObjRef, ObjectDef,
    ObjectKey, ObjectValue, ReloadOp, ReloadPatch, SourceSpan, TypeRef,
};
use ljos_reactive::NodeId;
use rustc_hash::FxHashMap as HashMap;

use super::{Phases, ReloadOutcome, resets};
use crate::engine::Engine;
use crate::ids::ObjectId;
use crate::incubator::{
    AllocFrame, Allocation, Creation, Instance, RootSpec, Template, is_component_attached,
};
use crate::loader::LoadedFile;
use crate::meta::{MetaRef, PropertyAccess};
use crate::object::QmlType;
use crate::qtqml::Component;
use crate::types::TypeTag;
use crate::value::Value;
use crate::{EngineError, PropIndex, WriteKind};

/// Where the dead instances and components are first swept out.
const FIRST_SWEEP: usize = 1024;

/// The code a binding, handler or method made while tracking runs.
pub(crate) type CodeSlot = RefCell<(Rc<Instance>, ExprId)>;

/// What a closure the incubator installs runs: an expression of an
/// instance, through a slot a reload in place points at the new file
/// while tracking.
#[derive(Clone)]
pub(crate) enum Code {
    Fixed(Rc<Instance>, ExprId),
    Slot(Rc<CodeSlot>),
}

impl Code {
    /// The instance and expression to run now. Not borrowed while it
    /// runs: a handler may reload.
    pub(crate) fn get(&self) -> (Rc<Instance>, ExprId) {
        match self {
            Code::Fixed(i, e) => (i.clone(), *e),
            Code::Slot(s) => s.borrow().clone(),
        }
    }
}

/// The members the edit gives an object of a changed file anew.
struct Edit {
    comp: CompIdx,
    /// Its index in the new file, and in the old.
    obj: usize,
    was: usize,
    names: Vec<String>,
    functions: Vec<String>,
}

/// What an op edits of an object.
enum Edited<'a> {
    Property(&'a str),
    Handler,
    Function(&'a str),
}

/// A changed file that is loaded: its old and new forms and the edits.
struct Changed {
    old: Rc<LoadedFile>,
    new: Arc<CompiledFile>,
    edits: Vec<Edit>,
    /// (component, object, alias name): an alias that stands for a whole
    /// edited object. A user that sets `name.member` through it outranks
    /// the file.
    whole: Vec<(CompIdx, usize, String)>,
    /// Per expression of the old file, the one at its place in the new.
    remap: Vec<Option<ExprId>>,
    /// The places of the old file that moved to where the new file has
    /// them.
    moved: Moves,
    /// Per component, where the old objects are in the new file.
    objects: Vec<Objects>,
}

/// Where the objects of a component of the old file are in the new one.
/// An object is the same one when the compiler gives it the same key.
struct Objects {
    /// Per old object, its index in the new file; `None` when removed.
    to_new: Vec<Option<usize>>,
    /// Per new object, its index in the old file; `None` when added.
    to_old: Vec<Option<usize>>,
    /// (old, new): one of the same key and type made again elsewhere,
    /// which goes on with what the old one was given.
    carry: Vec<(usize, usize)>,
}

impl Objects {
    fn same(&self) -> bool {
        self.to_new.len() == self.to_old.len()
            && self.to_new.iter().enumerate().all(|(i, n)| *n == Some(i))
    }

    /// The added objects written in one that stays, by new index.
    fn added_tops<'a>(&'a self, comp: &'a ComponentDef) -> impl Iterator<Item = usize> + 'a {
        (0..self.to_old.len()).filter(move |&j| {
            self.to_old[j].is_none()
                && comp.objects[j]
                    .parent
                    .is_some_and(|p| self.to_old[p.index()].is_some())
        })
    }

    /// The removed objects that were written in one that stays, by old
    /// index.
    fn removed_tops<'a>(&'a self, comp: &'a ComponentDef) -> impl Iterator<Item = usize> + 'a {
        (0..self.to_new.len()).filter(move |&i| {
            self.to_new[i].is_none()
                && comp.objects[i]
                    .parent
                    .is_some_and(|p| self.to_new[p.index()].is_some())
        })
    }
}

/// An instance an edit adds objects to or removes them from: made again
/// around the objects that stay.
struct Grown {
    old: Rc<Instance>,
    new: Rc<Instance>,
    cr: Creation,
    /// The added objects written in one that stays, by new index.
    tops: Vec<usize>,
    /// The removed objects that were written in one that stays.
    gone: Vec<ObjectId>,
    /// An id came or went: what the others bind to may have.
    ids: bool,
    /// (old, new): a moved object and the one made for it.
    carry: Vec<(ObjectId, ObjectId)>,
}

/// (line, column) of the old file to (line, column) of the new.
type Moves = HashMap<(u32, u32), (u32, u32)>;

/// An object of a live instance the edit is applied to.
struct Target<'a> {
    inst: Rc<Instance>,
    /// Its index in the new file, and in the old.
    obj: usize,
    was: usize,
    def: &'a ObjectDef,
    names: &'a [String],
    functions: &'a [String],
}

impl Engine {
    /// Notes the instance a creation made and those it is made of.
    pub(super) fn reload_note_instances(&self, top: &Rc<Instance>) {
        let st = &self.inner.reload;
        let mut all = st.instances.borrow_mut();
        let mut stack = vec![top.clone()];
        while let Some(inst) = stack.pop() {
            stack.extend(inst.inner.iter().flatten().cloned());
            all.push(inst);
        }
        let due = all.len() >= st.sweep_at.get().max(FIRST_SWEEP);
        drop(all);
        if due {
            self.reload_sweep();
        }
    }

    pub(crate) fn reload_note_template(&self, component: ObjectId) {
        let st = &self.inner.reload;
        if !st.tracking.get() {
            return;
        }
        let mut all = st.templates.borrow_mut();
        all.push(component);
        let due = all.len() >= st.sweep_at.get().max(FIRST_SWEEP);
        drop(all);
        if due {
            self.reload_sweep();
        }
    }

    /// What a binding, handler or method made now runs: through a slot
    /// while tracking.
    pub(crate) fn reload_code(&self, inst: &Rc<Instance>, expr: ExprId) -> Code {
        let st = &self.inner.reload;
        if !st.tracking.get() {
            return Code::Fixed(inst.clone(), expr);
        }
        let slot = Rc::new(RefCell::new((inst.clone(), expr)));
        let mut all = st.slots.borrow_mut();
        all.push(Rc::downgrade(&slot));
        let due = all.len() >= st.sweep_at.get().max(FIRST_SWEEP);
        drop(all);
        if due {
            self.reload_sweep();
        }
        Code::Slot(slot)
    }

    pub(crate) fn reload_note_given(&self, root: ObjectId, props: &[(String, Value)]) {
        let st = &self.inner.reload;
        if !st.tracking.get() || props.is_empty() {
            return;
        }
        let mut given = st.given.borrow_mut();
        let names = given.entry(root).or_default();
        for (name, _) in props {
            if !names.contains(name) {
                names.push(name.clone());
            }
        }
    }

    /// Forgets the instances and components whose objects are gone. Runs
    /// when the lists have doubled since the last sweep, so it costs a
    /// constant per instance made.
    pub(super) fn reload_sweep(&self) {
        let st = &self.inner.reload;
        let mut all = st.instances.borrow_mut();
        all.retain(|i| i.objects.first().is_some_and(|o| self.is_alive(*o)));
        let mut templates = st.templates.borrow_mut();
        templates.retain(|c| self.is_alive(*c));
        let mut slots = st.slots.borrow_mut();
        slots.retain(|h| h.strong_count() > 0);
        let most = all.len().max(templates.len()).max(slots.len());
        st.sweep_at.set((2 * most).max(FIRST_SWEEP));
    }

    /// Applies `patch` to the objects where they stand when it only gives
    /// them other values; `None` when the roots are to be made again.
    pub(super) fn reload_in_place(
        &self,
        patch: &ReloadPatch,
        roots: &[ObjectId],
    ) -> Option<ReloadOutcome> {
        let st = &self.inner.reload;
        if !patch.removed.is_empty() {
            return not_here("a file was removed");
        }
        if self.application_fault().is_some() {
            return not_here("the application has a fault");
        }
        let mut phases = Phases::start();
        let changed = self.in_place_files(patch)?;
        self.reload_sweep();
        let instances: Vec<Rc<Instance>> = st.instances.borrow().clone();
        // A file that asks for a reset starts afresh at every reload.
        if instances.iter().any(|i| resets(&i.file.ir)) {
            return not_here("a file asks for a reset");
        }
        // An instance used as the type of an object (`Foo {}`), to that
        // object's instance and index: what the user writes there
        // outranks the file.
        let mut users: HashMap<*const Instance, (Rc<Instance>, usize)> = HashMap::default();
        for inst in &instances {
            for (i, inner) in inst.inner.iter().enumerate() {
                if let Some(inner) = inner {
                    users.insert(Rc::as_ptr(inner), (inst.clone(), i));
                }
            }
        }
        let mut targets: Vec<Target> = Vec::new();
        for c in &changed {
            for inst in instances.iter().filter(|i| Rc::ptr_eq(&i.file, &c.old)) {
                for e in c.edits.iter().filter(|e| e.comp == inst.comp) {
                    let t = Target {
                        inst: inst.clone(),
                        obj: e.obj,
                        was: e.was,
                        def: &c.new.component(e.comp).objects[e.obj],
                        names: &e.names,
                        functions: &e.functions,
                    };
                    if let Err(why) = self.in_place_applies(&t, c, &users) {
                        return not_here(why);
                    }
                    targets.push(t);
                }
            }
        }
        // The instances objects are added to or removed from.
        let mut growing: Vec<(Rc<Instance>, usize)> = Vec::new();
        for inst in &instances {
            let Some(c) = changed.iter().position(|c| Rc::ptr_eq(&inst.file, &c.old)) else {
                continue;
            };
            if changed[c].objects[inst.comp.index()].same() {
                continue;
            }
            if !self.in_place_grows(inst, roots, &users) {
                tracing::debug!(
                    "reload not in place: {} is grown where it is made again",
                    inst.file.ir.file
                );
                return None;
            }
            growing.push((inst.clone(), c));
        }
        phases.mark("plan");

        let saved = self.reload_patch_loader(patch, false).ok()?;
        st.epoch.set(st.epoch.get() + 1);
        let mut files: HashMap<*const LoadedFile, Rc<LoadedFile>> = HashMap::default();
        for c in &changed {
            match self.load_file(&c.old.key) {
                Ok(new) => {
                    // The declarations are the same: so are their types,
                    // at the places the objects now have.
                    *new.metas.borrow_mut() = c
                        .old
                        .metas
                        .borrow()
                        .iter()
                        .filter_map(|(&(comp, obj), meta)| {
                            let to = c.objects.get(comp as usize)?.to_new.get(obj as usize)?;
                            Some(((comp, (*to)? as u32), meta.clone()))
                        })
                        .collect();
                    // A handler warned of once is not warned of again.
                    let warned = c.old.injected_warned.borrow();
                    new.injected_warned.borrow_mut().extend(
                        warned
                            .iter()
                            .filter_map(|e| c.remap.get(*e as usize).copied().flatten())
                            .map(|e| e.0),
                    );
                    drop(warned);
                    files.insert(Rc::as_ptr(&c.old), new);
                }
                Err(_) => {
                    // The full reload tells what is wrong.
                    st.epoch.set(st.epoch.get() - 1);
                    self.reload_restore_loader(saved);
                    return not_here("a file does not load");
                }
            }
        }
        phases.mark("loader");

        let mut memo: HashMap<*const Instance, Rc<Instance>> = HashMap::default();
        // Each instance is made again around the objects that stay, the
        // ones used as types and the ones a row is made in first, so that
        // the instances that hold them hold the new ones.
        let mut grown: Vec<Grown> = Vec::new();
        let errors = self.open_creation();
        let mut failed = false;
        for (inst, c) in grow_order(&growing) {
            let c = &changed[c];
            let Some(file) = files.get(&Rc::as_ptr(&c.old)) else {
                continue;
            };
            match self.in_place_grow(inst, file, &c.objects[inst.comp.index()], &files, &mut memo) {
                Some(g) => grown.push(g),
                None => {
                    failed = true;
                    break;
                }
            }
        }
        self.close_creation(errors);
        if failed {
            // What was added goes, and the full reload makes it and
            // tells what is wrong.
            for g in &grown {
                for &o in g.cr.all.iter().rev() {
                    if self.is_alive(o) {
                        self.destroy_now(o);
                    }
                }
            }
            st.epoch.set(st.epoch.get() - 1);
            self.reload_restore_loader(saved);
            return not_here("what is added cannot be made");
        }
        drop(saved);
        phases.mark("grow");
        self.batch(|| {
            for t in &targets {
                let inst = renew(&t.inst, &files, &mut memo);
                let id = inst.objects[t.obj];
                if !self.is_alive(id) {
                    continue;
                }
                for name in t.names {
                    self.in_place_set(&inst, id, t.def, name);
                }
                for f in t
                    .def
                    .functions
                    .iter()
                    .filter(|f| t.functions.contains(&f.name))
                {
                    self.install_function(&inst, id, f);
                }
            }
        });
        self.in_place_carry(&grown);
        for g in &grown {
            for &o in &g.gone {
                if self.is_alive(o) {
                    self.tell_removed(o);
                    self.destroy_now(o);
                }
            }
        }
        phases.mark("apply");

        // What is made from now on is made from the new files.
        for inst in st.instances.borrow_mut().iter_mut() {
            *inst = renew(inst, &files, &mut memo);
        }
        // And what is run: every binding, handler and method of an
        // instance of a changed file.
        let remaps: HashMap<*const LoadedFile, &[Option<ExprId>]> = changed
            .iter()
            .map(|c| (Rc::as_ptr(&c.old), c.remap.as_slice()))
            .collect();
        // An instance of a file that did not change is renewed too when
        // what is in it was: its slots must name the instance the next
        // reload finds, or that reload makes another from the old one.
        for slot in st.slots.borrow().iter().filter_map(std::rc::Weak::upgrade) {
            let (inst, expr) = slot.borrow().clone();
            let to = match remaps.get(&Rc::as_ptr(&inst.file)) {
                Some(r) => match r.get(expr.index()) {
                    Some(Some(to)) => *to,
                    _ => continue,
                },
                None => expr,
            };
            let now = renew(&inst, &files, &mut memo);
            if to != expr || !Rc::ptr_eq(&now, &inst) {
                *slot.borrow_mut() = (now, to);
            }
        }
        // A binding that names an id that came or went, once
        // the slots name the objects of the new instances: those of the
        // instance, and of a delegate or template made inside it, which
        // sees its ids.
        let named: Vec<&Rc<Instance>> = grown.iter().filter(|g| g.ids).map(|g| &g.new).collect();
        if !named.is_empty() {
            let sees = |i: &Instance| {
                let mut cur = Some(i);
                while let Some(c) = cur {
                    if named
                        .iter()
                        .any(|n| Rc::ptr_eq(&n.file, &c.file) && n.comp == c.comp)
                    {
                        return true;
                    }
                    cur = c.outer.as_deref();
                }
                false
            };
            let instances: Vec<Rc<Instance>> = st.instances.borrow().clone();
            self.batch(|| {
                for inst in instances.iter().filter(|i| sees(i)) {
                    for &id in &inst.objects {
                        if self.is_alive(id) {
                            self.in_place_refresh(id);
                        }
                    }
                }
            });
        }
        let added: rustc_hash::FxHashSet<ObjectId> = grown
            .iter()
            .flat_map(|g| g.cr.all.iter().copied())
            .collect();
        self.in_place_relocate(&changed, &added);
        // A binding that calls an edited function gives what it gives now.
        // Only those of the function's own file are run again.
        let called: Vec<*const LoadedFile> = changed
            .iter()
            .filter(|c| c.edits.iter().any(|e| !e.functions.is_empty()))
            .filter_map(|c| files.get(&Rc::as_ptr(&c.old)).map(Rc::as_ptr))
            .collect();
        if !called.is_empty() {
            let instances: Vec<Rc<Instance>> = st.instances.borrow().clone();
            self.batch(|| {
                for inst in instances
                    .iter()
                    .filter(|i| called.contains(&Rc::as_ptr(&i.file)))
                {
                    for &id in &inst.objects {
                        self.in_place_refresh(id);
                    }
                }
            });
        }
        let renew_template = |t: &Rc<Template>, memo: &mut HashMap<_, _>| -> Option<Rc<Template>> {
            let file = files.get(&Rc::as_ptr(&t.file)).cloned();
            let outer = t.outer.as_ref().map(|o| renew(o, &files, memo));
            let moved = outer
                .as_ref()
                .zip(t.outer.as_ref())
                .is_some_and(|(n, o)| !Rc::ptr_eq(n, o));
            if file.is_none() && !moved {
                return None;
            }
            Some(Rc::new(Template {
                file: file.unwrap_or_else(|| t.file.clone()),
                comp: t.comp,
                outer,
                context: t.context,
            }))
        };
        let components: Vec<ObjectId> = st.templates.borrow().clone();
        for c in components {
            let Ok(Some(t)) = self.with::<Component, _>(c, |c| c.template.clone()) else {
                continue;
            };
            if let Some(new) = renew_template(&t, &mut memo) {
                let _ = self.with_mut::<Component, _>(c, |c| c.template = Some(new));
            }
        }
        for s in st.scripted.borrow_mut().values_mut() {
            if let Some(new) = renew_template(&s.template, &mut memo) {
                s.template = new;
            }
        }
        phases.mark("templates");

        // The added objects complete as made objects do, and are known by
        // their places from now on.
        let mut tops: Vec<Rc<Instance>> = Vec::new();
        for g in &grown {
            self.in_place_complete(g);
            let mut top = g.old.clone();
            while let Some((user, _)) = users.get(&Rc::as_ptr(&top)) {
                top = user.clone();
            }
            let top = renew(&top, &files, &mut memo);
            if !tops.iter().any(|t| Rc::ptr_eq(t, &top)) {
                tops.push(top);
            }
        }
        for top in &tops {
            self.reload_repath(top);
        }
        phases.mark("grown");

        let under = self.under_roots(roots);
        // Every object went on: each is told so, as a full reload tells
        // the objects it keeps.
        let instances: Vec<Rc<Instance>> = st.instances.borrow().clone();
        for inst in &instances {
            let def = inst.file.ir.component(inst.comp);
            for (i, od) in def.objects.iter().enumerate() {
                let id = inst.objects[i];
                for at in od.attached.iter().filter(|a| is_component_attached(&a.ty)) {
                    for h in at
                        .members
                        .handlers
                        .iter()
                        .filter(|h| h.signal == "reloaded")
                    {
                        if self.is_alive(id) && under(id) && !added.contains(&id) {
                            self.call_expr(inst, h.expr, &[]);
                        }
                    }
                }
            }
        }
        phases.mark("reloaded");
        let kept = st
            .info
            .borrow()
            .keys()
            .filter(|o| self.is_alive(**o) && under(**o) && !added.contains(*o))
            .count();
        st.generation.set(patch.generation);
        *st.last_error.borrow_mut() = None;
        phases.mark("count");
        phases.done();
        Some(ReloadOutcome {
            generation: patch.generation,
            roots: roots.iter().map(|r| (*r, *r)).collect(),
            kept,
            fresh: added.len(),
            remounted: 0,
        })
    }

    /// The changed files that are loaded, with what is edited in each;
    /// `None` when the patch does more than give members other values.
    fn in_place_files(&self, patch: &ReloadPatch) -> Option<Vec<Changed>> {
        let mut by_file: HashMap<&str, Vec<(&ObjectKey, Edited)>> = HashMap::default();
        // Per file, the old key of a moved object to its new one.
        let mut moves: HashMap<&str, HashMap<&ObjectKey, &ObjectKey>> = HashMap::default();
        for op in &patch.ops {
            if let ReloadOp::ObjectMoved { file, from, to } = op {
                moves.entry(file.as_str()).or_default().insert(from, to);
                continue;
            }
            let (file, object, what) = match op {
                ReloadOp::PropertyChanged {
                    file,
                    object,
                    property,
                } if !property.contains('.') => (file, object, Edited::Property(property)),
                ReloadOp::HandlerChanged { file, object, .. } => (file, object, Edited::Handler),
                ReloadOp::DeclarationChanged { file, object, name } => {
                    (file, object, Edited::Function(name))
                }
                // Which objects came and went is read from the files: an
                // object is the same one when it has the same key, type
                // and parent, and keeps its order among its siblings. One
                // moved or retyped is removed and added.
                ReloadOp::ObjectAdded { .. }
                | ReloadOp::ObjectRemoved { .. }
                | ReloadOp::TypeChanged { .. } => continue,
                _ => {
                    tracing::debug!("reload not in place: {op:?}");
                    return None;
                }
            };
            by_file
                .entry(file.as_str())
                .or_default()
                .push((object, what));
        }
        let mut out = Vec::new();
        for f in &patch.files {
            let new = &f.file;
            if f.added {
                return not_here(format_args!("{} is new", new.file));
            }
            if resets(new) {
                return not_here(format_args!("{} asks for a reset", new.file));
            }
            if new.pragmas.iter().any(|p| p.name == "Singleton") {
                return not_here(format_args!("{} is a singleton", new.file));
            }
            let old = self.inner.loader.borrow().files.get(&new.file).cloned();
            // Nothing is made of it.
            let Some(old) = old else { continue };
            let mut at: HashMap<ObjectKey, (CompIdx, usize)> = HashMap::default();
            for (c, comp) in new.components.iter().enumerate() {
                for i in 0..comp.objects.len() {
                    let r = ObjRef {
                        component: CompIdx(c as u32),
                        object: ObjIdx(i as u32),
                    };
                    at.insert(new.object_key(r), (r.component, i));
                }
            }
            let mut edits: Vec<Edit> = Vec::new();
            for (key, what) in by_file.get(new.file.as_str()).into_iter().flatten() {
                let (comp, obj) = *at.get(key)?;
                let i = match edits.iter().position(|e| e.comp == comp && e.obj == obj) {
                    Some(i) => i,
                    None => {
                        edits.push(Edit {
                            comp,
                            obj,
                            was: obj,
                            names: Vec::new(),
                            functions: Vec::new(),
                        });
                        edits.len() - 1
                    }
                };
                match what {
                    Edited::Property(name) => edits[i].names.push((*name).to_owned()),
                    Edited::Function(name) => edits[i].functions.push((*name).to_owned()),
                    // Its slot runs the new body: nothing to install.
                    Edited::Handler => {}
                }
            }
            let Some((whole, remap, moved, objects)) = same_but_edits(
                &old.ir,
                new,
                &mut edits,
                moves.get(new.file.as_str()).unwrap_or(&HashMap::default()),
            ) else {
                tracing::debug!("reload not in place: {} changed its shape", new.file);
                return None;
            };
            out.push(Changed {
                old,
                new: Arc::new(new.clone()),
                edits,
                whole,
                remap,
                moved,
                objects,
            });
        }
        Some(out)
    }

    /// Whether objects can be added to `inst` and removed from it where it
    /// stands: it is of the tree a reload tracks. A row of a view, or what
    /// a `Loader` made from its `sourceComponent`, is tracked by itself.
    fn in_place_grows(
        &self,
        inst: &Rc<Instance>,
        roots: &[ObjectId],
        users: &HashMap<*const Instance, (Rc<Instance>, usize)>,
    ) -> bool {
        let mut cur = inst.clone();
        while let Some((user, _)) = users.get(&Rc::as_ptr(&cur)) {
            cur = user.clone();
        }
        // One made from a file's address (a page a `StackView` pushed) is
        // made once, as a root is, and the next one is made from the file
        // as it is now.
        cur.objects
            .first()
            .is_some_and(|r| roots.contains(r) || self.inner.reload.info.borrow().contains_key(r))
    }

    /// Makes `old` again from `file` around the objects that stay: the
    /// added ones are made and given what the file writes, under the
    /// object they are written in. `None`, with nothing made left, when
    /// one cannot be.
    fn in_place_grow(
        &self,
        old: &Rc<Instance>,
        file: &Rc<LoadedFile>,
        m: &Objects,
        files: &HashMap<*const LoadedFile, Rc<LoadedFile>>,
        memo: &mut HashMap<*const Instance, Rc<Instance>>,
    ) -> Option<Grown> {
        let comp = file.ir.component(old.comp);
        let was = old.file.ir.component(old.comp);
        let tops: Vec<usize> = m.added_tops(comp).collect();
        // One that goes before a sibling needs a list that can put it
        // there; the others are appended, as a creation does.
        for &j in &tops {
            let od = &comp.objects[j];
            let (Some(p), None) = (od.parent, &od.on) else {
                continue;
            };
            if anchor(comp, m, j).is_none() {
                continue;
            }
            let i = m.to_old[p.index()]?;
            let takes = self
                .list_of(file, &comp.objects[p.index()], j, old.objects[i])
                .is_ok_and(|prop| self.list_takes_insert(old.objects[i], prop));
            if !takes {
                tracing::debug!(
                    "reload not in place: {} puts an object where its parent cannot",
                    file.ir.file
                );
                return None;
            }
        }
        let mut frame = AllocFrame::new(
            file.clone(),
            old.comp,
            RootSpec::Existing(old.objects[0]),
            old.outer.as_ref().map(|o| renew(o, files, memo)),
            old.context,
        );
        frame.given = m
            .to_old
            .iter()
            .map(|o| {
                o.map(|i| {
                    let inner = old.inner[i].as_ref().map(|x| renew(x, files, memo));
                    (old.objects[i], inner)
                })
            })
            .collect();
        let mut a = Allocation { stack: vec![frame] };
        let mut cr = Creation::default();
        let undo = |cr: &Creation| {
            for &o in cr.all.iter().rev() {
                if self.is_alive(o) {
                    self.destroy_now(o);
                }
            }
        };
        let new = loop {
            match self.allocate_step(&mut cr, &mut a) {
                Ok(Some(inst)) => break inst,
                Ok(None) => {}
                Err(_) => {
                    undo(&cr);
                    return None;
                }
            }
        };
        memo.insert(Rc::as_ptr(old), new.clone());
        cr.top = Some(new.clone());
        for j in (0..comp.objects.len()).filter(|&j| m.to_old[j].is_none()) {
            if let Some(inner) = &new.inner[j] {
                self.reload_note_instances(inner);
            }
        }
        self.batch(|| {
            let ((), held) = self.hold_eager(|| {
                for &j in &tops {
                    let before = anchor(comp, m, j).map(|k| new.objects[k]);
                    self.in_place_attach(&new, j, before);
                }
                self.apply_aliases(&cr);
                for i in 0..cr.units.len() {
                    self.apply_unit(&cr, i);
                }
            });
            cr.held = held;
            self.run_held(&cr);
        });
        self.release_held(&cr);
        if self
            .check_required(&cr)
            .and_then(|()| self.check_assignable())
            .is_err()
        {
            undo(&cr);
            return None;
        }
        let gone: Vec<usize> = m.removed_tops(was).collect();
        let ids = tops.iter().any(|&j| comp.objects[j].id.is_some())
            || (0..was.objects.len()).any(|i| m.to_new[i].is_none() && was.objects[i].id.is_some())
            || (0..comp.objects.len())
                .any(|j| m.to_old[j].is_none() && comp.objects[j].id.is_some());
        Some(Grown {
            old: old.clone(),
            gone: gone.into_iter().map(|i| old.objects[i]).collect(),
            carry: m
                .carry
                .iter()
                .map(|&(i, j)| (old.objects[i], new.objects[j]))
                .collect(),
            new,
            cr,
            tops,
            ids,
        })
    }

    /// Puts the added object `j` of `inst` where the file writes it: on
    /// the default property of the object it is written in, before
    /// `before` or after what that holds, or on the property it operates
    /// on.
    fn in_place_attach(&self, inst: &Rc<Instance>, j: usize, before: Option<ObjectId>) {
        let comp = inst.file.ir.component(inst.comp);
        let od = &comp.objects[j];
        let Some(p) = od.parent else { return };
        let (parent, child) = (inst.objects[p.index()], inst.objects[j]);
        if let Some(on) = &od.on {
            self.apply_value_source(parent, child, on);
            return;
        }
        let r = self
            .list_of(&inst.file, &comp.objects[p.index()], j, parent)
            .and_then(
                |prop| match before.or_else(|| self.first_part(parent, prop)) {
                    Some(a) => self.list_insert(parent, prop, child, a).map(drop),
                    None => self.list_append(parent, prop, child),
                },
            );
        if let Err(e) = r {
            self.warn(format!(
                "{}Cannot assign to non-existent default property ({e})",
                self.location_of(child).unwrap_or_default()
            ));
        }
    }

    /// The first object in `parent`'s list `prop` that one of its native
    /// properties holds (a control's `background`, `contentItem`): a
    /// control takes those in when it completes, after what the file
    /// writes in it, so an object the file adds last goes before them.
    fn first_part(&self, parent: ObjectId, prop: PropIndex) -> Option<ObjectId> {
        let meta = self.meta_of(parent)?;
        self.untrack(|| {
            let Value::List(items) = self.read(parent, prop) else {
                return None;
            };
            let parts: Vec<ObjectId> = meta
                .properties
                .iter()
                .enumerate()
                .filter(|(_, p)| {
                    matches!(p.access, PropertyAccess::Field { .. })
                        && matches!(p.ty, TypeTag::Object(_))
                        && !p.is_alias()
                })
                .filter_map(|(i, _)| self.read(parent, PropIndex(i as u32)).as_object())
                .collect();
            items
                .iter()
                .filter_map(Value::as_object)
                .find(|o| parts.contains(o))
        })
    }

    /// The list property of `parent` (written as `def`) the object `j` is
    /// written in.
    fn list_of(
        &self,
        file: &Rc<LoadedFile>,
        def: &ObjectDef,
        j: usize,
        parent: ObjectId,
    ) -> Result<PropIndex, EngineError> {
        match place(def, j).and_then(|(name, _, _)| name) {
            Some(name) => self.property_index(parent, name),
            None => self.children_default(file, &def.ty),
        }
    }

    /// What a creation does last, for the added objects of `g`: their
    /// types complete, the last made first, and then their
    /// `Component.onCompleted` runs.
    fn in_place_complete(&self, g: &Grown) {
        for &o in g.cr.all.iter().rev() {
            if self.is_alive(o)
                && let Err(e) = self.complete(o)
            {
                self.warn(e.to_string());
            }
        }
        let mut stack: Vec<ObjectId> = g.tops.iter().rev().map(|&j| g.new.objects[j]).collect();
        while let Some(o) = stack.pop() {
            let Some(node) = g.cr.nodes.get(&o) else {
                continue;
            };
            for (inst, expr) in &node.handlers {
                if self.is_alive(o) {
                    self.call_expr(inst, *expr, &[]);
                }
            }
            if let Some(att) = self.existing_attached_object(
                o,
                &MetaRef::Static(<Component as QmlType>::static_meta()),
            ) {
                let _ = self.emit_by_name(att, "completed", &[]);
            }
            stack.extend(node.children.iter().copied());
        }
    }

    /// Whether the edit can be applied to `t` as it stands: nothing that
    /// outranks the file sets the edited members.
    fn in_place_applies(
        &self,
        t: &Target,
        c: &Changed,
        users: &HashMap<*const Instance, (Rc<Instance>, usize)>,
    ) -> Result<(), &'static str> {
        let id = t.inst.objects[t.was];
        if !self.is_alive(id) {
            return Ok(());
        }
        // A PropertyChanges or a Binding keeps what it is given for its
        // target.
        if self.foreign_keeper(id).is_some() {
            return Err("a state or a Binding holds the edited object");
        }
        let given = self.inner.reload.given.borrow();
        let given = given.get(&id);
        let old = &c.old.ir.component(t.inst.comp).objects[t.was];
        for name in t.names {
            let Ok(target) = self.untrack(|| self.member_target(id, &[], name)) else {
                return Err("an edited member is not found");
            };
            // A member the file did not give may be held by what took it
            // over (a view sizes a page that has no size of its own),
            // which decides again when the object is made again.
            if !sets(old, name) && self.overridden(target.obj, target.prop) {
                return Err("an edited member is held by what took it over");
            }
            if given.is_some_and(|g| g.iter().any(|n| is_or_under(n, name))) {
                return Err("an edited member is given where the object is made");
            }
        }
        let wholes: Vec<&str> = c
            .whole
            .iter()
            .filter(|(comp, obj, _)| *comp == t.inst.comp && *obj == t.obj)
            .map(|(.., alias)| alias.as_str())
            .collect();
        // Up the objects that use the component as their type, while the
        // edited object is the root of the one below. Only the first sees
        // the aliases of the component's root.
        let mut below = t.inst.clone();
        let mut root = t.obj == 0;
        let mut first = true;
        while let Some((user, i)) = users.get(&Rc::as_ptr(&below)) {
            let ud = &user.file.ir.component(user.comp).objects[*i];
            // Its own function of the name is the one the object has.
            if root && ud.functions.iter().any(|f| t.functions.contains(&f.name)) {
                return Err("a user of the component has its own function of the name");
            }
            for name in t.names {
                if root && (sets(ud, name) || aliased(&user.file.ir, user.comp, *i, name)) {
                    return Err("a user of the component sets the edited member");
                }
                if first && wholes.iter().any(|a| sets_through(ud, a, name)) {
                    return Err("a user of the component sets the edited member by an alias");
                }
            }
            first = false;
            root = root && *i == 0;
            if !root {
                break;
            }
            below = user.clone();
        }
        Ok(())
    }

    fn overridden(&self, obj: ObjectId, prop: crate::PropIndex) -> bool {
        let (obj, prop) = self.redirect(obj, prop);
        let node = self
            .inner
            .store
            .borrow()
            .objects
            .get(obj)
            .and_then(|h| h.node(prop));
        node.is_some_and(|n| self.inner.graph.override_count(n) > 0)
    }

    /// Gives `obj` the member `name` as the new file writes it.
    fn in_place_set(&self, inst: &Rc<Instance>, obj: ObjectId, od: &ObjectDef, name: &str) {
        let Ok(t) = self.untrack(|| self.member_target(obj, &[], name)) else {
            return;
        };
        if let Some(a) = od.members.assignments.iter().find(|a| a.name == name) {
            if matches!(a.value, Literal::Undefined) {
                if let Err(e) =
                    self.write_internal(t.obj, t.prop, Value::Undefined, WriteKind::Initial)
                {
                    let (line, col) = (a.span.line, a.span.column);
                    self.warn(format!("{}:{line}:{col}: {e}", inst.file.url));
                }
            } else {
                self.apply_literal(inst, &t, &a.value, (a.span.line, a.span.column));
            }
        } else if let Some(pb) = od.members.bindings.iter().find(|b| b.name == name) {
            self.install_binding(inst, &t, pb);
        }
        // The edit outranks what a script assigned there.
        let (o, p) = self.redirect(t.obj, t.prop);
        if let Some(props) = self.inner.reload.assigned.borrow_mut().get_mut(&o) {
            props.retain(|q| *q != p);
        }
    }

    /// Tells the objects of the changed files, and their bindings, where
    /// they are now written.
    fn in_place_relocate(&self, changed: &[Changed], added: &rustc_hash::FxHashSet<ObjectId>) {
        let moved: HashMap<&str, &Moves> = changed
            .iter()
            .filter(|c| !c.moved.is_empty())
            .map(|c| (c.old.url.as_str(), &c.moved))
            .collect();
        if moved.is_empty() {
            return;
        }
        if let Some(js) = self.inner.js.borrow().as_ref() {
            let mut lines = js.lines.borrow_mut();
            for c in changed.iter().filter(|c| !c.moved.is_empty()) {
                lines.moved(&c.old.url, &c.new, &c.moved);
            }
        }
        let shift = |l: &mut crate::object::Location| {
            if let Some(&(line, column)) = moved
                .get(l.url.as_str())
                .and_then(|m| m.get(&(l.line, l.column)))
            {
                l.line = line;
                l.column = column;
            }
        };
        let mut store = self.inner.store.borrow_mut();
        for (id, h) in store.objects.iter_mut() {
            // Made where the new file writes it.
            if added.contains(&id) {
                continue;
            }
            let Some(ext) = h.ext.as_deref_mut() else {
                continue;
            };
            if let Some(l) = &mut ext.location {
                shift(l);
            }
            for b in &mut ext.binding_locs {
                shift(&mut b.at);
            }
        }
    }

    /// Gives each moved object's new one what scripts and its state gave
    /// the old one, before the old one goes.
    fn in_place_carry(&self, grown: &[Grown]) {
        let st = &self.inner.reload;
        if grown.iter().all(|g| g.carry.is_empty()) {
            return;
        }
        let state_props = st.state_props.borrow().clone();
        let migrators = st.migrators.borrow().clone();
        let was_muting = (st.muting.get(), st.muted_all.get());
        st.muting.set(true);
        st.muted_all.set(true);
        self.batch(|| {
            for &(from, to) in grown.iter().flat_map(|g| &g.carry) {
                if !self.is_alive(from) || !self.is_alive(to) {
                    continue;
                }
                if let Some(taken) = self.reload_take(from, &state_props, &migrators) {
                    self.reload_give(to, taken, &Default::default(), &migrators);
                }
            }
        });
        st.muting.set(was_muting.0);
        st.muted_all.set(was_muting.1);
    }

    /// Runs the bindings of `obj` again, keeping where each came from.
    fn in_place_refresh(&self, obj: ObjectId) {
        let nodes: Vec<NodeId> = {
            let store = self.inner.store.borrow();
            match store.objects.get(obj).and_then(|h| h.nodes.as_ref()) {
                Some(n) => n.iter().flatten().copied().collect(),
                None => return,
            }
        };
        for n in nodes {
            let _ = self.inner.graph.refresh(n);
        }
    }

    /// Whether an object is one of `roots` or owned by one, remembered per
    /// object asked about.
    fn under_roots<'a>(&'a self, roots: &'a [ObjectId]) -> impl Fn(ObjectId) -> bool + 'a {
        let seen: std::cell::RefCell<HashMap<ObjectId, bool>> = Default::default();
        move |o: ObjectId| {
            let mut walked = Vec::new();
            let mut cur = Some(o);
            let found = loop {
                let Some(c) = cur else { break false };
                if let Some(v) = seen.borrow().get(&c) {
                    break *v;
                }
                if roots.contains(&c) {
                    break true;
                }
                walked.push(c);
                cur = self.owner(c);
            };
            let mut seen = seen.borrow_mut();
            for w in walked {
                seen.insert(w, found);
            }
            found
        }
    }
}

/// `growing` in the order it is grown in: an instance after those it
/// holds, by `inner` (one used as a type) or by `outer` (the instance a
/// row is made in), however deep.
fn grow_order(growing: &[(Rc<Instance>, usize)]) -> Vec<(&Rc<Instance>, usize)> {
    fn visit<'a>(
        inst: &Rc<Instance>,
        at: &HashMap<*const Instance, (&'a Rc<Instance>, usize)>,
        seen: &mut rustc_hash::FxHashSet<*const Instance>,
        out: &mut Vec<(&'a Rc<Instance>, usize)>,
    ) {
        if !seen.insert(Rc::as_ptr(inst)) {
            return;
        }
        for held in inst.outer.iter().chain(inst.inner.iter().flatten()) {
            visit(held, at, seen, out);
        }
        if let Some(&g) = at.get(&Rc::as_ptr(inst)) {
            out.push(g);
        }
    }
    let at: HashMap<*const Instance, (&Rc<Instance>, usize)> = growing
        .iter()
        .map(|(i, c)| (Rc::as_ptr(i), (i, *c)))
        .collect();
    let mut seen = rustc_hash::FxHashSet::default();
    let mut out = Vec::with_capacity(growing.len());
    for (inst, _) in growing {
        visit(inst, &at, &mut seen, &mut out);
    }
    out
}

/// `inst` with the files of `files` for theirs, and so its outer and
/// inner instances; `inst` itself when none of them changed.
fn renew(
    inst: &Rc<Instance>,
    files: &HashMap<*const LoadedFile, Rc<LoadedFile>>,
    memo: &mut HashMap<*const Instance, Rc<Instance>>,
) -> Rc<Instance> {
    if let Some(r) = memo.get(&Rc::as_ptr(inst)) {
        return r.clone();
    }
    let outer = inst.outer.as_ref().map(|o| renew(o, files, memo));
    let inner: Vec<Option<Rc<Instance>>> = inst
        .inner
        .iter()
        .map(|i| i.as_ref().map(|i| renew(i, files, memo)))
        .collect();
    let file = files.get(&Rc::as_ptr(&inst.file)).cloned();
    let same = |a: &Option<Rc<Instance>>, b: &Option<Rc<Instance>>| match (a, b) {
        (Some(a), Some(b)) => Rc::ptr_eq(a, b),
        (None, None) => true,
        _ => false,
    };
    let unchanged = file.is_none()
        && same(&outer, &inst.outer)
        && inner.iter().zip(&inst.inner).all(|(a, b)| same(a, b));
    let r = if unchanged {
        inst.clone()
    } else {
        Rc::new(Instance {
            file: file.unwrap_or_else(|| inst.file.clone()),
            comp: inst.comp,
            objects: inst.objects.clone(),
            inner,
            outer,
            context: inst.context,
        })
    };
    memo.insert(Rc::as_ptr(inst), r.clone());
    r
}

/// `given` names `name` or a member of it (`font.bold` of `font`).
fn is_or_under(given: &str, name: &str) -> bool {
    given == name
        || given
            .strip_prefix(name)
            .is_some_and(|rest| rest.starts_with('.'))
}

/// Whether `od` gives its object `name` in any way.
fn sets(od: &ObjectDef, name: &str) -> bool {
    let m = &od.members;
    m.assignments.iter().any(|a| is_or_under(&a.name, name))
        || m.bindings.iter().any(|b| is_or_under(&b.name, name))
        || m.objects.iter().any(|o| o.name == name)
        || od
            .grouped
            .iter()
            .any(|g| g.path.first().is_some_and(|p| p == name))
}

/// Whether `od` sets `name` of what its alias `alias` stands for:
/// `label.text: ...` or `label { text: ... }`.
fn sets_through(od: &ObjectDef, alias: &str, name: &str) -> bool {
    let dotted = format!("{alias}.{name}");
    let m = &od.members;
    m.assignments.iter().any(|a| is_or_under(&a.name, &dotted))
        || m.bindings.iter().any(|b| is_or_under(&b.name, &dotted))
        || od.grouped.iter().any(|g| {
            g.path.first().is_some_and(|p| p == alias)
                && (g.path.get(1).is_some_and(|p| p == name)
                    || (g.path.len() == 1 && members_set(&g.members, name)))
        })
}

fn members_set(m: &Members, name: &str) -> bool {
    m.assignments.iter().any(|a| is_or_under(&a.name, name))
        || m.bindings.iter().any(|b| is_or_under(&b.name, name))
        || m.objects.iter().any(|o| o.name == name)
}

/// Whether an alias of `file` stands for `name` of object `obj` of `comp`.
fn aliased(file: &CompiledFile, comp: CompIdx, obj: usize, name: &str) -> bool {
    let at = ObjRef {
        component: comp,
        object: ObjIdx(obj as u32),
    };
    file.components
        .iter()
        .flat_map(|c| &c.objects)
        .flat_map(|o| &o.properties)
        .filter_map(|p| p.alias.as_ref())
        .any(|a| a.object == Some(at) && a.path.first().is_some_and(|p| p == name))
}

/// What [`same_but_edits`] finds: the aliases of whole edited objects
/// (see [`Changed::whole`]), where the old expressions are in the new
/// file, the places that moved (see [`Changed::moved`]) and where the
/// objects are (see [`Changed::objects`]).
type Same = (
    Vec<(CompIdx, usize, String)>,
    Vec<Option<ExprId>>,
    Moves,
    Vec<Objects>,
);

/// [`Same`] when `new` is `old` but for the edited members and for
/// objects added after the children of one that stays or removed;
/// `None` otherwise. Sets the old index of each edit.
fn same_but_edits(
    old: &CompiledFile,
    new: &CompiledFile,
    edits: &mut Vec<Edit>,
    moves: &HashMap<&ObjectKey, &ObjectKey>,
) -> Option<Same> {
    if old.imports != new.imports
        || old.pragmas != new.pragmas
        || old.scripts != new.scripts
        || old.invocable != new.invocable
        || old.components.len() != new.components.len()
    {
        return not_here("its imports, pragmas, scripts or components changed");
    }
    let mut objects = Vec::with_capacity(old.components.len());
    for (c, (o, n)) in old.components.iter().zip(&new.components).enumerate() {
        if o.name != n.name || o.id != n.id || o.kind != n.kind || o.signature != n.signature {
            return not_here("a component's name, id, kind or signature changed");
        }
        let m = match_objects(old, new, CompIdx(c as u32), moves)?;
        if !m.same() && !grows_simply(o, n, &m) {
            return not_here("objects came or went where they cannot be put");
        }
        objects.push(m);
    }
    // An added object is made from the new file as a whole.
    edits.retain(|e| objects[e.comp.index()].to_old[e.obj].is_some());
    for e in edits.iter_mut() {
        e.was = objects[e.comp.index()].to_old[e.obj]?;
        // A list of objects in both: what it holds is matched as objects
        // are, and the shapes below tell whether it only grew.
        let listed = |od: &ObjectDef, name: &str| {
            od.members
                .objects
                .iter()
                .any(|oa| oa.is_list && oa.name == name)
        };
        let (o, n) = (
            &old.component(e.comp).objects[e.was],
            &new.component(e.comp).objects[e.obj],
        );
        e.names.retain(|name| !(listed(o, name) && listed(n, name)));
    }
    let none: Vec<String> = Vec::new();
    let mut remap = vec![None; old.exprs.len()];
    let mut moved = HashMap::default();
    let (mut was, mut now) = (Vec::new(), Vec::new());
    let (mut here, mut there) = (Vec::new(), Vec::new());
    for (c, (o, n)) in old.components.iter().zip(&new.components).enumerate() {
        let m = &objects[c];
        for (i, oo) in o.objects.iter().enumerate() {
            let Some(j) = m.to_new[i] else { continue };
            let no = &n.objects[j];
            let names = edits
                .iter()
                .find(|e| e.comp.index() == c && e.obj == j)
                .map_or(&none, |e| &e.names);
            let mut a = shape(oo, names);
            let mut b = shape(no, names);
            if !m.same() {
                renumber(&mut a, c, &objects);
                let added = |v: &ObjectValue| matches!(v, ObjectValue::Object(k) if m.to_old[k.index()].is_none());
                b.children.retain(|v| !added(v));
                for oa in b.members.objects.iter_mut().filter(|oa| oa.is_list) {
                    oa.values.retain(|v| !added(v));
                }
            }
            if a != b {
                return not_here(format_args!("an object's {} changed", differs(&a, &b)));
            }
            was.clear();
            now.clear();
            exprs_of(oo, names, &mut was);
            exprs_of(no, names, &mut now);
            if was.len() != now.len() {
                return not_here("an object has other expressions");
            }
            for (w, n) in was.iter().zip(&now) {
                *remap.get_mut(w.index())? = Some(*n);
                here.push(old.expr(*w).span);
                there.push(new.expr(*n).span);
            }
            places(oo, names, &mut here);
            places(no, names, &mut there);
            if here.len() != there.len() {
                return not_here("an object has other places");
            }
            for (h, t) in here.drain(..).zip(there.drain(..)) {
                if (h.line, h.column) != (t.line, t.column) {
                    moved.insert((h.line, h.column), (t.line, t.column));
                }
            }
        }
    }
    let mut whole = Vec::new();
    for e in edits.iter() {
        let o = &old.component(e.comp).objects[e.was];
        let n = &new.component(e.comp).objects[e.obj];
        let parent = n.parent.map(|p| &new.component(e.comp).objects[p.index()]);
        if is_model(n) || parent.is_some_and(is_model) {
            return not_here("a model's elements changed");
        }
        for name in &e.names {
            let given = |m: &Members| {
                m.assignments.iter().any(|a| a.name == *name)
                    || m.bindings.iter().any(|b| b.name == *name)
            };
            let written = |m: &Members| m.objects.iter().any(|a| a.name == *name);
            // A member no longer given goes back to its default, which
            // nothing here knows.
            if !given(&n.members) {
                return not_here("a member is no longer given");
            }
            if written(&n.members) || written(&o.members) {
                return not_here("a member given an object changed");
            }
            if aliased(new, e.comp, e.obj, name) {
                return not_here("an aliased member changed");
            }
        }
        // A function edited in body alone: the object's method stays.
        for name in &e.functions {
            let args = |od: &ObjectDef, f: &CompiledFile| {
                od.functions
                    .iter()
                    .find(|d| d.name == *name)
                    .map(|d| f.expr(d.expr).args.clone())
            };
            match (args(o, old), args(n, new)) {
                (Some(a), Some(b)) if a == b => {}
                _ => return not_here("a function's parameters changed"),
            }
        }
        let at = ObjRef {
            component: e.comp,
            object: ObjIdx(e.obj as u32),
        };
        // Users see the aliases of the component's root alone.
        for p in &new.component(e.comp).objects[0].properties {
            if let Some(a) = &p.alias
                && a.object == Some(at)
                && a.path.is_empty()
            {
                whole.push((e.comp, e.obj, p.name.clone()));
            }
        }
    }
    Some((whole, remap, moved, objects))
}

/// None, telling why at debug level: the reload does not go in place.
fn not_here<T>(why: impl std::fmt::Display) -> Option<T> {
    tracing::debug!("reload not in place: {why}");
    None
}

/// What part of an object's shape `a` and `b` differ in.
fn differs(a: &ObjectDef, b: &ObjectDef) -> &'static str {
    if a.properties != b.properties {
        "declared properties"
    } else if a.signals != b.signals {
        "signals"
    } else if a.functions != b.functions {
        "functions"
    } else if a.members.objects != b.members.objects {
        "members given objects"
    } else if a.children != b.children {
        "children"
    } else if a.members != b.members {
        "members"
    } else if a.grouped != b.grouped {
        "grouped members"
    } else if a.attached != b.attached {
        "attached members"
    } else if a.enums != b.enums || a.required != b.required {
        "enums or required members"
    } else {
        "shape"
    }
}

/// A model's elements are its data, read once.
fn is_model(od: &ObjectDef) -> bool {
    matches!(&od.ty, TypeRef::Native { name, .. } if name == "ListElement" || name == "ListModel")
}

/// Pairs the objects of component `comp` of both files by their keys;
/// `None` when two objects of a file have one key.
fn match_objects(
    old: &CompiledFile,
    new: &CompiledFile,
    comp: CompIdx,
    moves: &HashMap<&ObjectKey, &ObjectKey>,
) -> Option<Objects> {
    let key = |f: &CompiledFile, i: usize| {
        f.object_key(ObjRef {
            component: comp,
            object: ObjIdx(i as u32),
        })
    };
    let (o, n) = (old.component(comp), new.component(comp));
    let mut at: HashMap<ObjectKey, usize> = HashMap::default();
    for j in 0..n.objects.len() {
        if at.insert(key(new, j), j).is_some() {
            return not_here("two objects have one key");
        }
    }
    let mut to_new = Vec::with_capacity(o.objects.len());
    let mut to_old = vec![None; n.objects.len()];
    let mut keyed = Vec::new();
    let mut moved = Vec::new();
    for i in 0..o.objects.len() {
        let k = key(old, i);
        let j = at.remove(&k);
        keyed.extend(j.map(|j| (i, j)));
        if j.is_none()
            && let Some(to) = moves.get(&k)
        {
            moved.push((i, (*to).clone()));
        }
        if let Some(j) = j {
            to_old[j] = Some(i);
        }
        to_new.push(j);
    }
    // Two old objects of one key: the second finds it taken.
    if to_new.iter().flatten().count() != to_old.iter().flatten().count() {
        return not_here("two old objects have one key");
    }
    // One of another type, or under another parent, is another object, and
    // so is all that is in it. A parent comes before what is in it.
    for j in 0..n.objects.len() {
        let Some(i) = to_old[j] else { continue };
        let parent = match (n.objects[j].parent, o.objects[i].parent) {
            (Some(p), Some(q)) => to_old[p.index()] == Some(q.index()),
            (None, None) => true,
            _ => false,
        };
        if !parent || n.objects[j].ty != o.objects[i].ty || n.objects[j].on != o.objects[i].on {
            to_old[j] = None;
            to_new[i] = None;
        }
    }
    // Siblings in another order: the most of them that keep theirs stay,
    // the others are made again with all that is in them.
    for values in n.objects.iter().flat_map(lists) {
        let kept: Vec<(usize, usize)> = values
            .iter()
            .filter_map(|v| match v {
                ObjectValue::Object(k) => to_old[k.index()].map(|i| (k.index(), i)),
                ObjectValue::Component(_) => None,
            })
            .collect();
        let was: Vec<usize> = kept.iter().map(|&(_, i)| i).collect();
        for (&(j, i), stays) in kept.iter().zip(longest_rising(&was)) {
            if !stays {
                to_old[j] = None;
                to_new[i] = None;
            }
        }
    }
    for j in 0..n.objects.len() {
        if let Some(i) = to_old[j]
            && n.objects[j]
                .parent
                .is_some_and(|p| to_old[p.index()].is_none())
        {
            to_old[j] = None;
            to_new[i] = None;
        }
    }
    // One moved keeps its key: it is made again, but what was given to it
    // goes on, as the full path carries it.
    // One the compiler saw move to another key is found under it.
    for (i, to) in moved {
        if let Some(j) = at.remove(&to) {
            keyed.push((i, j));
        }
    }
    keyed.sort_unstable();
    // One under an object of another type starts afresh, as all below
    // that does. A parent comes before what is in it.
    let mut carried = vec![false; o.objects.len()];
    let mut carry = Vec::new();
    for (i, j) in keyed {
        let under = o.objects[i]
            .parent
            .is_none_or(|p| to_new[p.index()].is_some() || carried[p.index()]);
        if to_new[i].is_none()
            && under
            && n.objects[j].ty == o.objects[i].ty
            && n.objects[j].on == o.objects[i].on
        {
            carried[i] = true;
            carry.push((i, j));
        }
    }
    Some(Objects {
        to_new,
        to_old,
        carry,
    })
}

/// The lists of objects `od` holds in order: its children, and each list
/// given to a member.
fn lists(od: &ObjectDef) -> impl Iterator<Item = &[ObjectValue]> {
    let listed = od
        .members
        .objects
        .iter()
        .filter(|oa| oa.is_list)
        .map(|oa| oa.values.as_slice());
    std::iter::once(od.children.as_slice()).chain(listed)
}

/// Which of `seq` make up a longest rising run of it, in its order.
fn longest_rising(seq: &[usize]) -> Vec<bool> {
    // Quadratic, over the children of one object.
    let mut len = vec![1usize; seq.len()];
    let mut prev = vec![usize::MAX; seq.len()];
    for b in 0..seq.len() {
        for a in 0..b {
            if seq[a] < seq[b] && len[a] + 1 > len[b] {
                len[b] = len[a] + 1;
                prev[b] = a;
            }
        }
    }
    let mut stay = vec![false; seq.len()];
    let mut at = (0..seq.len()).max_by_key(|&k| (len[k], usize::MAX - k));
    while let Some(k) = at {
        stay[k] = true;
        at = (prev[k] != usize::MAX).then_some(prev[k]);
    }
    stay
}

/// Where `j` is written in `parent`: among the children of its default
/// property (no name) or in a list given to the member named, those
/// values, and its place among them.
fn place(parent: &ObjectDef, j: usize) -> Option<(Option<&str>, &[ObjectValue], usize)> {
    let me = ObjectValue::Object(ObjIdx(j as u32));
    if let Some(at) = parent.children.iter().position(|v| *v == me) {
        return Some((None, &parent.children, at));
    }
    parent.members.objects.iter().find_map(|oa| {
        let at = oa.values.iter().position(|v| *v == me)?;
        oa.is_list
            .then_some((Some(oa.name.as_str()), oa.values.as_slice(), at))
    })
}

/// The object that stays and comes right after the added object `j`
/// where its parent holds it, which `j` is put before.
fn anchor(comp: &ComponentDef, m: &Objects, j: usize) -> Option<usize> {
    let parent = &comp.objects[comp.objects[j].parent?.index()];
    let (_, values, at) = place(parent, j)?;
    values[at + 1..].iter().find_map(|v| match v {
        ObjectValue::Object(k) if m.to_old[k.index()].is_some() => Some(k.index()),
        _ => None,
    })
}

/// Whether the objects of `n` are those of `o` with objects added after
/// the children of one that stays and objects removed, in the same order
/// otherwise: what a reload in place can make and destroy.
fn grows_simply(o: &ComponentDef, n: &ComponentDef, m: &Objects) -> bool {
    // The root is the instance's own object.
    if m.to_new.first() != Some(&Some(0)) {
        return false;
    }
    let kept: Vec<usize> = m.to_new.iter().flatten().copied().collect();
    if kept.windows(2).any(|w| w[0] > w[1]) {
        return false;
    }
    for j in m.added_tops(n) {
        let od = &n.objects[j];
        let Some(p) = od.parent else { return false };
        let parent = &n.objects[p.index()];
        if is_model(od) || is_model(parent) {
            return false;
        }
        // A child of the default property or of a list a member is
        // given, put before the object that stays after it, if one does. A
        // `Component` written there is not an object of the instance to
        // put it before.
        let Some((_, values, at)) = place(parent, j) else {
            return false;
        };
        if values[at..]
            .iter()
            .any(|v| matches!(v, ObjectValue::Component(_)))
        {
            return false;
        }
    }
    for i in m.removed_tops(o) {
        let od = &o.objects[i];
        if od.parent.is_some_and(|p| is_model(&o.objects[p.index()])) || is_model(od) {
            return false;
        }
    }
    true
}

/// `od` of the old file with the indices the new file gives its objects,
/// and without the children that are removed. One that has none there
/// (a removed object given to a member) is told apart by an index no
/// object has.
fn renumber(od: &mut ObjectDef, comp: usize, objects: &[Objects]) {
    let to = |c: usize, i: ObjIdx| -> Option<ObjIdx> {
        objects[c].to_new[i.index()].map(|j| ObjIdx(j as u32))
    };
    let gone = ObjIdx(u32::MAX);
    od.parent = od.parent.map(|p| to(comp, p).unwrap_or(gone));
    od.children.retain_mut(|v| match v {
        ObjectValue::Object(i) => match to(comp, *i) {
            Some(j) => {
                *i = j;
                true
            }
            None => false,
        },
        ObjectValue::Component(_) => true,
    });
    let values = |m: &mut Members| {
        for oa in &mut m.objects {
            for v in &mut oa.values {
                if let ObjectValue::Object(i) = v {
                    *i = to(comp, *i).unwrap_or(gone);
                }
            }
            // A list goes on without what was removed from it, as the
            // children do.
            if oa.is_list {
                oa.values.retain(|v| *v != ObjectValue::Object(gone));
            }
        }
    };
    values(&mut od.members);
    for g in &mut od.grouped {
        values(&mut g.members);
    }
    for a in &mut od.attached {
        values(&mut a.members);
    }
    for p in &mut od.properties {
        if let Some(r) = p.alias.as_mut().and_then(|a| a.object.as_mut()) {
            r.object = to(r.component.index(), r.object).unwrap_or(gone);
        }
    }
}

/// The expressions of `od` in the order both files are walked in, but
/// for the bindings of the edited `names`, which have none to match.
fn exprs_of(od: &ObjectDef, names: &[String], out: &mut Vec<ExprId>) {
    fn members(m: &Members, skip: &[String], out: &mut Vec<ExprId>) {
        out.extend(
            m.bindings
                .iter()
                .filter(|b| !skip.contains(&b.name))
                .map(|b| b.expr),
        );
        out.extend(m.handlers.iter().map(|h| h.expr));
    }
    members(&od.members, names, out);
    for g in &od.grouped {
        members(&g.members, &[], out);
    }
    for a in &od.attached {
        members(&a.members, &[], out);
    }
    out.extend(od.functions.iter().map(|f| f.expr));
}

/// The places `od` writes its object and members at, but for the edited
/// `names`, in the order both files are walked in.
fn places(od: &ObjectDef, names: &[String], out: &mut Vec<SourceSpan>) {
    fn members(m: &Members, skip: &[String], out: &mut Vec<SourceSpan>) {
        out.extend(
            m.assignments
                .iter()
                .filter(|a| !skip.contains(&a.name))
                .map(|a| a.span),
        );
        out.extend(
            m.bindings
                .iter()
                .filter(|b| !skip.contains(&b.name))
                .map(|b| b.name_span),
        );
        out.extend(m.handlers.iter().map(|h| h.name_span));
    }
    out.push(od.span);
    out.extend(od.properties.iter().map(|p| p.span));
    members(&od.members, names, out);
    for g in &od.grouped {
        members(&g.members, &[], out);
    }
    for a in &od.attached {
        members(&a.members, &[], out);
    }
}

/// `od` without the members `names`, with no places (the objects are
/// told where they now are) and with no expressions (a value made a
/// binding moves those after it; the patch tells which changed).
fn shape(od: &ObjectDef, names: &[String]) -> ObjectDef {
    let mut o = od.clone();
    o.members.assignments.retain(|a| !names.contains(&a.name));
    o.members.bindings.retain(|b| !names.contains(&b.name));
    for f in &mut o.functions {
        f.expr = ExprId(0);
    }
    flatten(&mut o.span);
    for p in &mut o.properties {
        flatten(&mut p.span);
    }
    flatten_members(&mut o.members);
    for g in &mut o.grouped {
        flatten_members(&mut g.members);
    }
    for a in &mut o.attached {
        flatten_members(&mut a.members);
    }
    o
}

fn flatten_members(m: &mut Members) {
    for a in &mut m.assignments {
        flatten(&mut a.span);
    }
    for b in &mut m.bindings {
        flatten(&mut b.name_span);
        b.expr = ExprId(0);
    }
    for h in &mut m.handlers {
        flatten(&mut h.name_span);
        h.expr = ExprId(0);
    }
}

fn flatten(s: &mut SourceSpan) {
    *s = SourceSpan::default();
}
