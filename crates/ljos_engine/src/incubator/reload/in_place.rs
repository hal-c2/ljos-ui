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
//! objects, declarations and lines. Then what is in effect is the edit
//! alone. Anything else (an object added, a group's member, a property no
//! longer given, a signal) is made again by [`Engine::reload`].
//!
//! The objects keep the bindings they have, which run the old file's
//! code: that code is the same. The edited members and functions are
//! installed from the new file, and so are the `Component`s, so that a
//! delegate made later is made from the new file. A handler connected
//! while tracking runs through a [`HandlerSlot`], which is pointed at the
//! new file: an edited body runs on the next signal. A value made a
//! binding moves the expressions after it, so an old expression is found
//! in the new file by its place.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use ljos_ir::{
    CompIdx, CompiledFile, ExprId, Literal, Members, ObjIdx, ObjRef, ObjectDef, ObjectKey,
    ReloadOp, ReloadPatch, SourceSpan, TypeRef,
};
use ljos_reactive::NodeId;
use rustc_hash::FxHashMap as HashMap;

use super::{Phases, ReloadOutcome, resets};
use crate::WriteKind;
use crate::engine::Engine;
use crate::ids::ObjectId;
use crate::incubator::{Instance, Template, is_component_attached};
use crate::loader::LoadedFile;
use crate::qtqml::Component;
use crate::value::Value;

/// Where the dead instances and components are first swept out.
const FIRST_SWEEP: usize = 1024;

/// The code a handler connected while tracking runs.
pub(crate) type HandlerSlot = RefCell<(Rc<Instance>, ExprId)>;

/// The members the edit gives an object of a changed file anew.
struct Edit {
    comp: CompIdx,
    obj: usize,
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
}

/// An object of a live instance the edit is applied to.
struct Target<'a> {
    inst: Rc<Instance>,
    obj: usize,
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

    /// The slot a handler made now runs through; `None` when not tracking.
    pub(crate) fn reload_note_handler(
        &self,
        inst: &Rc<Instance>,
        expr: ExprId,
    ) -> Option<Rc<HandlerSlot>> {
        let st = &self.inner.reload;
        if !st.tracking.get() {
            return None;
        }
        let slot = Rc::new(RefCell::new((inst.clone(), expr)));
        let mut all = st.handlers.borrow_mut();
        all.push(Rc::downgrade(&slot));
        let due = all.len() >= st.sweep_at.get().max(FIRST_SWEEP);
        drop(all);
        if due {
            self.reload_sweep();
        }
        Some(slot)
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
        let mut handlers = st.handlers.borrow_mut();
        handlers.retain(|h| h.strong_count() > 0);
        let most = all.len().max(templates.len()).max(handlers.len());
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
        if !patch.removed.is_empty() || self.application_fault().is_some() {
            return None;
        }
        let mut phases = Phases::start();
        let changed = self.in_place_files(patch)?;
        self.reload_sweep();
        let instances: Vec<Rc<Instance>> = st.instances.borrow().clone();
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
                        def: &c.new.component(e.comp).objects[e.obj],
                        names: &e.names,
                        functions: &e.functions,
                    };
                    if !self.in_place_applies(&t, c, &users) {
                        return None;
                    }
                    targets.push(t);
                }
            }
        }
        phases.mark("plan");

        let saved = self.reload_patch_loader(patch, false).ok()?;
        st.epoch.set(st.epoch.get() + 1);
        let mut files: HashMap<*const LoadedFile, Rc<LoadedFile>> = HashMap::default();
        for c in &changed {
            match self.load_file(&c.old.key) {
                Ok(new) => {
                    // The declarations are the same: so are their types.
                    *new.metas.borrow_mut() = c.old.metas.borrow().clone();
                    files.insert(Rc::as_ptr(&c.old), new);
                }
                Err(_) => {
                    // The full reload tells what is wrong.
                    st.epoch.set(st.epoch.get() - 1);
                    self.reload_restore_loader(saved);
                    return None;
                }
            }
        }
        drop(saved);
        phases.mark("loader");

        let mut memo: HashMap<*const Instance, Rc<Instance>> = HashMap::default();
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
        phases.mark("apply");

        // What is made from now on is made from the new files.
        for inst in st.instances.borrow_mut().iter_mut() {
            *inst = renew(inst, &files, &mut memo);
        }
        // And what is run: every handler of an instance of a changed file.
        let remaps: HashMap<*const LoadedFile, &[Option<ExprId>]> = changed
            .iter()
            .map(|c| (Rc::as_ptr(&c.old), c.remap.as_slice()))
            .collect();
        for slot in st
            .handlers
            .borrow()
            .iter()
            .filter_map(std::rc::Weak::upgrade)
        {
            let (inst, expr) = slot.borrow().clone();
            let Some(Some(to)) = remaps
                .get(&Rc::as_ptr(&inst.file))
                .and_then(|r| r.get(expr.index()))
            else {
                continue;
            };
            *slot.borrow_mut() = (renew(&inst, &files, &mut memo), *to);
        }
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
                        if self.is_alive(id) && under(id) {
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
            .filter(|o| self.is_alive(**o) && under(**o))
            .count();
        st.generation.set(patch.generation);
        *st.last_error.borrow_mut() = None;
        phases.mark("count");
        phases.done();
        Some(ReloadOutcome {
            generation: patch.generation,
            roots: roots.iter().map(|r| (*r, *r)).collect(),
            kept,
            fresh: 0,
            remounted: 0,
        })
    }

    /// The changed files that are loaded, with what is edited in each;
    /// `None` when the patch does more than give members other values.
    fn in_place_files(&self, patch: &ReloadPatch) -> Option<Vec<Changed>> {
        let mut by_file: HashMap<&str, Vec<(&ObjectKey, Edited)>> = HashMap::default();
        for op in &patch.ops {
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
                _ => return None,
            };
            by_file
                .entry(file.as_str())
                .or_default()
                .push((object, what));
        }
        let mut out = Vec::new();
        for f in &patch.files {
            let new = &f.file;
            if f.added || resets(new) || new.pragmas.iter().any(|p| p.name == "Singleton") {
                return None;
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
            let (whole, remap) = same_but_edits(&old.ir, new, &edits)?;
            out.push(Changed {
                old,
                new: Arc::new(new.clone()),
                edits,
                whole,
                remap,
            });
        }
        Some(out)
    }

    /// Whether the edit can be applied to `t` as it stands: nothing that
    /// outranks the file sets the edited members.
    fn in_place_applies(
        &self,
        t: &Target,
        c: &Changed,
        users: &HashMap<*const Instance, (Rc<Instance>, usize)>,
    ) -> bool {
        let id = t.inst.objects[t.obj];
        if !self.is_alive(id) {
            return true;
        }
        // A PropertyChanges or a Binding keeps what it is given for its
        // target.
        if self.foreign_keeper(id).is_some() {
            return false;
        }
        let given = self.inner.reload.given.borrow();
        let given = given.get(&id);
        for name in t.names {
            if self.untrack(|| self.member_target(id, &[], name)).is_err() {
                return false;
            }
            if given.is_some_and(|g| g.iter().any(|n| is_or_under(n, name))) {
                return false;
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
                return false;
            }
            for name in t.names {
                if root && (sets(ud, name) || aliased(&user.file.ir, user.comp, *i, name)) {
                    return false;
                }
                if first && wholes.iter().any(|a| sets_through(ud, a, name)) {
                    return false;
                }
            }
            first = false;
            root = root && *i == 0;
            if !root {
                break;
            }
            below = user.clone();
        }
        true
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

/// The aliases of whole edited objects (see [`Changed::whole`]) and where
/// the old expressions are in the new file, when `new` is `old` but for
/// the edited members; `None` otherwise.
#[allow(clippy::type_complexity)]
fn same_but_edits(
    old: &CompiledFile,
    new: &CompiledFile,
    edits: &[Edit],
) -> Option<(Vec<(CompIdx, usize, String)>, Vec<Option<ExprId>>)> {
    if old.imports != new.imports
        || old.pragmas != new.pragmas
        || old.scripts != new.scripts
        || old.invocable != new.invocable
        || old.components.len() != new.components.len()
    {
        return None;
    }
    let none: Vec<String> = Vec::new();
    let mut remap = vec![None; old.exprs.len()];
    let (mut was, mut now) = (Vec::new(), Vec::new());
    for (c, (o, n)) in old.components.iter().zip(&new.components).enumerate() {
        if o.name != n.name
            || o.id != n.id
            || o.kind != n.kind
            || o.signature != n.signature
            || o.objects.len() != n.objects.len()
        {
            return None;
        }
        for (i, (oo, no)) in o.objects.iter().zip(&n.objects).enumerate() {
            let names = edits
                .iter()
                .find(|e| e.comp.index() == c && e.obj == i)
                .map_or(&none, |e| &e.names);
            if shape(oo, names) != shape(no, names) {
                return None;
            }
            was.clear();
            now.clear();
            exprs_of(oo, names, &mut was);
            exprs_of(no, names, &mut now);
            if was.len() != now.len() {
                return None;
            }
            for (w, n) in was.iter().zip(&now) {
                *remap.get_mut(w.index())? = Some(*n);
            }
        }
    }
    let mut whole = Vec::new();
    for e in edits {
        let o = &old.component(e.comp).objects[e.obj];
        let n = &new.component(e.comp).objects[e.obj];
        // A model's elements are its data, read once.
        let model = |od: &ObjectDef| matches!(&od.ty, TypeRef::Native { name, .. } if name == "ListElement" || name == "ListModel");
        let parent = n.parent.map(|p| &new.component(e.comp).objects[p.index()]);
        if model(n) || parent.is_some_and(model) {
            return None;
        }
        for name in &e.names {
            let given = |m: &Members| {
                m.assignments.iter().any(|a| a.name == *name)
                    || m.bindings.iter().any(|b| b.name == *name)
            };
            let written = |m: &Members| m.objects.iter().any(|a| a.name == *name);
            // A member no longer given goes back to its default, which
            // nothing here knows.
            if !given(&n.members) || written(&n.members) || written(&o.members) {
                return None;
            }
            if aliased(new, e.comp, e.obj, name) {
                return None;
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
                _ => return None,
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
    Some((whole, remap))
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

/// `od` without the members `names`, with its places told by line alone
/// (an edit moves what follows it on its line) and with no expressions
/// (a value made a binding moves those after it; the patch tells which
/// changed).
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
    *s = SourceSpan {
        line: s.line,
        ..SourceSpan::default()
    };
}
