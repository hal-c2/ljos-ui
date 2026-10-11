//! The object creator (ARCHITECTURE.md §6.3): makes the objects of a
//! compiled component and wires them up.
//!
//! Creation runs in phases, so it can also run as a resumable
//! [`CreationJob`]:
//!
//! 1. **Allocate** every object of the component (and of the file
//!    components it uses, whose roots are the using objects), with a type
//!    made for each object that declares members, ids in a fresh context.
//! 2. **Apply**, inside one batch: aliases first, then per object its
//!    literals, grouped values, object values, children, handlers,
//!    functions, bindings and attached members. Bindings are lazy, except
//!    what an effect (a change handler) pulls; a component's own level is
//!    applied before the level that uses it. In eager mode the bindings
//!    run once all are in place, in the order Qt runs them
//!    (`Engine::run_held`), so a binding loop is reported for the same
//!    member as there.
//! 3. **Finalize**: initial properties, the required check, native
//!    completion hooks, then `Component.onCompleted` in Qt's order (each
//!    object before its children, children last first; a component's own
//!    handler before its user's).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use ljos_ir::{
    Arg, Attached, Binding, CompIdx, ComponentKind, ExprId, Literal, Members, ObjRef, ObjectValue,
    PropertyBinding, SourceSpan, TranslationKind, TypeRef,
};
use ljos_js::{JsValue, Scope};
use ljos_runtime::{Job, JobCx, JobPoll, Lane, ProcId};

use crate::atom::Atom;
use crate::engine::{Engine, Held};
use crate::error::EngineError;
use crate::ids::{ContextId, ObjectId, PropIndex};
use crate::loader::LoadedFile;
use crate::meta::{MetaBuilder, MetaRef, PropertyFlags};
use crate::object::{BindingLoc, Location, QmlType};
use crate::qtqml::{Component, Connections};
use crate::types::TypeTag;
use crate::value::{Color, Value};
use crate::{Provenance, WriteKind};

pub(crate) mod reload;
pub(crate) mod tier1;

/// Deepest nesting of file components; deeper is a component using
/// itself.
const MAX_DEPTH: u32 = 64;

/// The objects one component instance is made of.
pub(crate) struct Instance {
    pub(crate) file: Rc<LoadedFile>,
    pub(crate) comp: CompIdx,
    pub(crate) objects: Vec<ObjectId>,
    /// Per object: the instance of the component file it is made from
    /// (`Foo {}`), whose root it is.
    inner: Vec<Option<Rc<Instance>>>,
    /// The instance a template or delegate was made inside: its ids are in
    /// sight.
    pub(crate) outer: Option<Rc<Instance>>,
    pub(crate) context: ContextId,
}

impl Instance {
    fn resolve(&self, r: ObjRef) -> Option<ObjectId> {
        let mut cur = Some(self);
        while let Some(i) = cur {
            if i.comp == r.component {
                return i.objects.get(r.object.index()).copied();
            }
            cur = i.outer.as_deref();
        }
        None
    }

    fn find(&self, comp: CompIdx) -> Option<&Instance> {
        let mut cur = Some(self);
        while let Some(i) = cur {
            if i.comp == comp {
                return Some(i);
            }
            cur = i.outer.as_deref();
        }
        None
    }

    fn url(&self) -> &Atom {
        &self.file.url
    }
}

/// What a `Component` makes objects from.
pub(crate) struct Template {
    pub(crate) file: Rc<LoadedFile>,
    pub(crate) comp: CompIdx,
    pub(crate) outer: Option<Rc<Instance>>,
}

/// One object to apply: object `obj` of `inst`.
struct Unit {
    inst: Rc<Instance>,
    obj: usize,
}

/// `Component.onCompleted` handlers and the creation tree they run over.
#[derive(Default)]
struct CompletionNode {
    handlers: Vec<(Rc<Instance>, ExprId)>,
    /// `Component.onReloaded`: run instead of `handlers` for an object
    /// that a dev reload kept.
    reloaded: Vec<(Rc<Instance>, ExprId)>,
    children: Vec<ObjectId>,
}

#[derive(Default)]
struct Creation {
    units: Vec<Unit>,
    /// In allocation order.
    all: Vec<ObjectId>,
    nodes: HashMap<ObjectId, CompletionNode>,
    depth: u32,
    /// The instance the creation was asked for.
    top: Option<Rc<Instance>>,
    /// Eager mode: the bindings installed so far, not yet run.
    held: Vec<Held>,
}

/// Puts the hold of an outer creation back when an inner one is through,
/// also if it panics.
struct HoldGuard<'a> {
    engine: &'a Engine,
    outer: Option<Vec<Held>>,
}

impl Drop for HoldGuard<'_> {
    fn drop(&mut self) {
        *self.engine.inner.eager_hold.borrow_mut() = self.outer.take();
    }
}

enum RootSpec {
    New(Option<ObjectId>),
    /// The object that uses the component (`Foo {}`), already made.
    Existing(ObjectId),
}

fn instance_key(inst: &Instance) -> usize {
    std::ptr::from_ref(inst) as usize
}

/// The bindings of object `obj` of `inst` and of everything written in
/// it, as (instance, offset of the name), in the order Qt sets them up:
/// the component the object is made from first; then what the object is
/// given by name, from the last written to the first, an object value
/// set up where it stands; then the objects written in it, first to
/// last; then what it gives attached objects, and last its groups, each
/// from the last written to the first.
fn setup_order(inst: &Rc<Instance>, obj: usize, out: &mut Vec<(usize, u32)>) {
    if let Some(Some(inner)) = inst.inner.get(obj) {
        setup_order(inner, 0, out);
    }
    let def = inst.file.ir.component(inst.comp);
    let Some(od) = def.objects.get(obj) else {
        return;
    };
    members_order(inst, &od.members, out);
    for v in &od.children {
        if let ObjectValue::Object(j) = v {
            setup_order(inst, j.index(), out);
        }
    }
    let mut sets: Vec<&Members> = od.attached.iter().map(|a| &a.members).collect();
    sets.sort_by_key(|m| std::cmp::Reverse(first_binding(inst, m)));
    for m in sets {
        members_order(inst, m, out);
    }
    let mut sets: Vec<&Members> = od.grouped.iter().map(|g| &g.members).collect();
    sets.sort_by_key(|m| std::cmp::Reverse(first_binding(inst, m)));
    for m in sets {
        members_order(inst, m, out);
    }
}

/// The offset of the first binding of `m`.
fn first_binding(inst: &Instance, m: &Members) -> u32 {
    m.bindings
        .iter()
        .map(|pb| name_place(pb, inst.file.ir.expr(pb.expr).span).start)
        .min()
        .unwrap_or(u32::MAX)
}

/// [`setup_order`] for one set of members.
fn members_order(inst: &Rc<Instance>, m: &Members, out: &mut Vec<(usize, u32)>) {
    let def = inst.file.ir.component(inst.comp);
    // (offset, the objects given there; none for a binding)
    let mut steps: Vec<(u32, &[ObjectValue])> = m
        .bindings
        .iter()
        .map(|pb| {
            let at = name_place(pb, inst.file.ir.expr(pb.expr).span).start;
            (at, &[][..])
        })
        .collect();
    for oa in &m.objects {
        let at = oa.values.iter().find_map(|v| match v {
            ObjectValue::Object(j) => def.objects.get(j.index()).map(|o| o.span.start),
            ObjectValue::Component(_) => None,
        });
        if let Some(at) = at {
            steps.push((at, &oa.values));
        }
    }
    steps.sort_by_key(|s| std::cmp::Reverse(s.0));
    for (at, values) in steps {
        if values.is_empty() {
            out.push((instance_key(inst), at));
        }
        for v in values {
            if let ObjectValue::Object(j) = v {
                setup_order(inst, j.index(), out);
            }
        }
    }
}

/// Where Qt says a binding is: at its name. IR built by hand may leave
/// the name's place out; the expression's is the next best.
fn name_place(pb: &PropertyBinding, expr: SourceSpan) -> SourceSpan {
    if pb.name_span.line == 0 {
        expr
    } else {
        pb.name_span
    }
}

/// A property (or a dotted group leaf) to write.
struct Target {
    obj: ObjectId,
    prop: PropIndex,
}

impl Engine {
    /// Makes the file component at `url` (a [`crate::ComponentSource`]
    /// key, or `file:///key.qml`), owned by `parent`, with
    /// `initial_props` set before completion (they count for `required`).
    pub fn create_from_url(
        &self,
        url: &str,
        parent: Option<ObjectId>,
        initial_props: Vec<(String, Value)>,
    ) -> Result<ObjectId, EngineError> {
        let template = self.file_template(url)?;
        if !self.inner.reload.tracking.get() {
            return self.create_from_template(&template, parent, initial_props);
        }
        let root = self.create_from_template(&template, parent, initial_props.clone())?;
        self.reload_note_origin(root, url, parent, &initial_props);
        Ok(root)
    }

    /// A `Component` object for the file at `url` (`Qt.createComponent`).
    pub fn create_component(
        &self,
        url: &str,
        owner: Option<ObjectId>,
    ) -> Result<ObjectId, EngineError> {
        let template = self.file_template(url)?;
        self.make_component_object(owner, template)
    }

    fn file_template(&self, url: &str) -> Result<Template, EngineError> {
        let key = self.url_key(url);
        let file = self.load_file(&key)?;
        let comp = file
            .find_component(None)
            .ok_or_else(|| EngineError::Load(format!("{}: no component", file.url)))?;
        Ok(Template {
            file,
            comp,
            outer: None,
        })
    }

    pub(crate) fn create_from_template(
        &self,
        template: &Template,
        parent: Option<ObjectId>,
        props: Vec<(String, Value)>,
    ) -> Result<ObjectId, EngineError> {
        let mut cr = Creation::default();
        let root = self.allocate_template(&mut cr, template, parent)?;
        self.reload_assign(&cr, root);
        let r = self.batch(|| -> Result<(), EngineError> {
            let (r, held) = self.hold_eager(|| {
                self.apply_aliases(&cr);
                for i in 0..cr.units.len() {
                    self.apply_unit(&cr, i);
                }
                self.finish_props(root, &props)
            });
            cr.held = held;
            self.run_held(&cr);
            r
        });
        if r.is_ok() {
            self.release_held(&cr);
        }
        self.finalize(cr, root, r)
    }

    fn allocate_template(
        &self,
        cr: &mut Creation,
        template: &Template,
        parent: Option<ObjectId>,
    ) -> Result<ObjectId, EngineError> {
        self.allocate_template_as(cr, template, parent, true)
    }

    /// [`Engine::allocate_template`]; without `append` the root is owned
    /// by `parent` but is not a value of its default property (a view's
    /// delegates are not its children).
    fn allocate_template_as(
        &self,
        cr: &mut Creation,
        template: &Template,
        parent: Option<ObjectId>,
        append: bool,
    ) -> Result<ObjectId, EngineError> {
        if parent.is_some_and(|p| !self.is_alive(p)) {
            return Err(EngineError::DeadObject(parent.expect("checked")));
        }
        let ctx_parent = template
            .outer
            .as_ref()
            .map_or_else(|| self.root_context(), |o| o.context);
        let inst = self.allocate(
            cr,
            template.file.clone(),
            template.comp,
            RootSpec::New(parent),
            template.outer.clone(),
            ctx_parent,
        )?;
        let root = inst.objects[0];
        cr.top = Some(inst.clone());
        // The instance (and so its contexts) lives as long as its root.
        self.keep_with(root, inst.clone());
        if append
            && let Some(p) = parent
            && self.default_property(p).is_ok()
        {
            self.list_append_default(p, root)?;
        }
        Ok(root)
    }

    /// Makes one delegate of a view from the `Component` object
    /// `component`, owned by `owner`. `setup` gets the root and the
    /// delegate's context as soon as they exist and before anything in
    /// the delegate runs: the place to give the context its row and the
    /// root its visual parent.
    pub(crate) fn create_delegate(
        &self,
        component: ObjectId,
        owner: Option<ObjectId>,
        setup: impl FnOnce(&Engine, ObjectId, ContextId),
    ) -> Result<ObjectId, EngineError> {
        let template = self.component_template(component)?;
        let mut cr = Creation::default();
        let root = self.allocate_template_as(&mut cr, &template, owner, false)?;
        if let Some(top) = &cr.top {
            setup(self, root, top.context);
        }
        // After `setup`: a delegate's path has its row in it.
        self.reload_assign(&cr, root);
        self.batch(|| {
            let ((), held) = self.hold_eager(|| {
                self.apply_aliases(&cr);
                for i in 0..cr.units.len() {
                    self.apply_unit(&cr, i);
                }
            });
            cr.held = held;
            self.run_held(&cr);
        });
        self.release_held(&cr);
        self.finalize(cr, root, Ok(()))
    }

    /// Initial properties, then the required check.
    fn finish_props(&self, root: ObjectId, props: &[(String, Value)]) -> Result<(), EngineError> {
        for (name, v) in props {
            let p = self.property_index(root, name)?;
            self.write_internal(root, p, v.clone(), WriteKind::Imperative)?;
        }
        Ok(())
    }

    fn check_required(&self, cr: &Creation) -> Result<(), EngineError> {
        for &o in &cr.all {
            let Some(meta) = self.meta_of(o) else {
                continue;
            };
            for (i, p) in meta.properties.iter().enumerate() {
                if !p.flags.contains(PropertyFlags::REQUIRED) {
                    continue;
                }
                let prop = PropIndex(i as u32);
                let set = self.is_bound(o, prop)
                    || self
                        .provenance(o, prop)
                        .is_some_and(|p| p != Provenance::Default);
                if !set {
                    let at = self.location_of(o).unwrap_or_default();
                    return Err(EngineError::RequiredNotSet(format!(
                        "{at}Required property {} was not initialized",
                        p.name
                    )));
                }
            }
        }
        Ok(())
    }

    fn finalize(
        &self,
        cr: Creation,
        root: ObjectId,
        applied: Result<(), EngineError>,
    ) -> Result<ObjectId, EngineError> {
        if let Err(e) = applied.and_then(|()| self.check_required(&cr)) {
            self.reload_discard(root);
            return Err(e);
        }
        // Dev reload: the objects that go on for old ones take what
        // those held before anything completes, and are not completed
        // in the file's sense a second time.
        let kept = self.reload_adopt(&cr);
        // Native hooks: the last made first, so children are complete
        // before the objects that lay them out.
        for &o in cr.all.iter().rev() {
            if self.is_alive(o)
                && let Err(e) = self.complete(o)
            {
                self.warn(e.to_string());
            }
        }
        let mut stack = vec![root];
        while let Some(o) = stack.pop() {
            let Some(node) = cr.nodes.get(&o) else {
                continue;
            };
            if !kept.contains(&o) {
                for (inst, expr) in &node.handlers {
                    if self.is_alive(o) {
                        self.call_expr(inst, *expr, &[]);
                    }
                }
            }
            stack.extend(node.children.iter().copied());
        }
        if !kept.is_empty() {
            let mut stack = vec![root];
            while let Some(o) = stack.pop() {
                let Some(node) = cr.nodes.get(&o) else {
                    continue;
                };
                if kept.contains(&o) {
                    for (inst, expr) in &node.reloaded {
                        if self.is_alive(o) {
                            self.call_expr(inst, *expr, &[]);
                        }
                    }
                }
                stack.extend(node.children.iter().copied());
            }
        }
        Ok(root)
    }

    pub(crate) fn keep_with(&self, obj: ObjectId, data: Rc<dyn std::any::Any>) {
        if let Some(h) = self.inner.store.borrow_mut().objects.get_mut(obj) {
            h.ext_mut().keep.push(data);
        }
    }

    fn set_location(&self, obj: ObjectId, url: &Atom, line: u32, column: u32) {
        if let Some(h) = self.inner.store.borrow_mut().objects.get_mut(obj) {
            h.ext_mut().location = Some(Location {
                url: url.clone(),
                line,
                column,
            });
        }
    }

    /// Notes where the binding now on `prop` is written; an earlier
    /// binding's place (the base file's, say) is forgotten.
    fn set_binding_loc(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        inst: &Instance,
        at: SourceSpan,
        member: Option<Atom>,
    ) {
        if let Some(h) = self.inner.store.borrow_mut().objects.get_mut(obj) {
            let locs = &mut h.ext_mut().binding_locs;
            locs.retain(|l| l.prop != prop);
            locs.push(BindingLoc {
                prop,
                at: Location {
                    url: inst.url().clone(),
                    line: at.line,
                    column: at.column,
                },
                member,
            });
        }
        // The binding was installed just now, so it is at the end.
        let node = match self.inner.eager_hold.borrow_mut().as_mut() {
            Some(held) => held
                .iter_mut()
                .rev()
                .find(|h| h.obj == obj && h.prop == prop)
                .map(|h| {
                    h.place = Some((instance_key(inst), at.start));
                    h.node
                }),
            None => None,
        };
        // Qt installs this one after the others have run; until then
        // the property reads as what it was.
        if let Some(n) = node
            && self.is_qt_bindable(obj, prop)
        {
            let _ = self.inner.graph.hold(n);
        }
    }

    /// Runs `f` with eager evaluation held back and returns the bindings
    /// it installed. Nothing is held in lazy mode.
    fn hold_eager<R>(&self, f: impl FnOnce() -> R) -> (R, Vec<Held>) {
        if !self.inner.eager_bindings.get() {
            return (f(), Vec::new());
        }
        let outer = self.inner.eager_hold.replace(Some(Vec::new()));
        let guard = HoldGuard {
            engine: self,
            outer,
        };
        let r = f();
        let held = self.inner.eager_hold.borrow_mut().take();
        drop(guard);
        (r, held.unwrap_or_default())
    }

    /// Eager mode: runs the bindings a creation installed, in the order
    /// Qt runs them (found by running qml6 6.11.2 with bindings that log),
    /// so that the same member of a binding loop is reported and the
    /// members are left with the same values.
    ///
    /// Qt sets an object up in the order of [`setup_order`] and then runs
    /// the bindings from the last one set up to the first; a binding that
    /// reads a property whose binding has not run yet runs that one first.
    /// That is this function. Bindings on Qt's bindable properties are not
    /// there yet while this happens (the graph holds them), so a read gives
    /// the old value; [`Engine::release_held`] brings them in afterwards.
    fn run_held(&self, cr: &Creation) {
        let g = &self.inner.graph;
        for i in self.held_order(cr, false) {
            let n = cr.held[i].node;
            if g.is_bound(n) && g.set_eager(n, true).is_ok() {
                g.untrack(|| g.try_get(n));
            }
        }
    }

    /// The second half of [`Engine::run_held`]: Qt installs the bindings
    /// on its bindable properties one by one in set-up order and runs each
    /// as it goes in; what read the property before runs again. Called
    /// outside the creation's batch so that those run before the next
    /// binding goes in, as in Qt.
    fn release_held(&self, cr: &Creation) {
        let g = &self.inner.graph;
        for i in self.held_order(cr, true) {
            let n = cr.held[i].node;
            if !g.is_held(n) {
                continue;
            }
            g.batch(|| {
                if g.release(n).is_ok() && g.set_eager(n, true).is_ok() {
                    g.untrack(|| g.try_get(n));
                }
            });
        }
    }

    /// Indices into `cr.held`: the held bindable ones in set-up order, or
    /// the others from the last set up to the first.
    fn held_order(&self, cr: &Creation, bindable: bool) -> Vec<usize> {
        if cr.held.is_empty() {
            return Vec::new();
        }
        let mut places: HashMap<(usize, u32), usize> = HashMap::new();
        if let Some(top) = &cr.top {
            let mut order = Vec::new();
            setup_order(top, 0, &mut order);
            places = order.into_iter().enumerate().map(|(i, k)| (k, i)).collect();
        }
        // A binding replaced by a later one (the using file's) is gone.
        let mut last: HashMap<_, usize> = HashMap::new();
        for (i, h) in cr.held.iter().enumerate() {
            last.insert(h.node, i);
        }
        let g = &self.inner.graph;
        let mut run: Vec<(u8, usize, usize)> = cr
            .held
            .iter()
            .enumerate()
            .filter(|(i, h)| last[&h.node] == *i && g.is_held(h.node) == bindable)
            .map(|(i, h)| match h.place.and_then(|p| places.get(&p)) {
                Some(&at) if bindable => (0, at, i),
                Some(&at) => (0, usize::MAX - at, i),
                // Installed by a native type: after the written ones, in
                // the order they came.
                None => (1, 0, i),
            })
            .collect();
        run.sort_unstable();
        run.into_iter().map(|r| r.2).collect()
    }

    fn add_context(&self, obj: ObjectId, ctx: ContextId) {
        if let Some(h) = self.inner.store.borrow_mut().objects.get_mut(obj) {
            h.ext_mut().contexts.push(ctx);
        }
    }

    /// `url:line:col: ` of an object made from QML.
    pub(crate) fn location_of(&self, obj: ObjectId) -> Option<String> {
        let store = self.inner.store.borrow();
        let loc = store.objects.get(obj)?.ext.as_ref()?.location.as_ref()?;
        Some(format!("{}:{}:{}: ", loc.url, loc.line, loc.column))
    }

    // ---------------------------------------------------------- allocation

    fn allocate(
        &self,
        cr: &mut Creation,
        file: Rc<LoadedFile>,
        comp: CompIdx,
        root: RootSpec,
        outer: Option<Rc<Instance>>,
        ctx_parent: ContextId,
    ) -> Result<Rc<Instance>, EngineError> {
        cr.depth += 1;
        if cr.depth > MAX_DEPTH {
            return Err(EngineError::Load(format!(
                "{}: the component instantiates itself",
                file.url
            )));
        }
        let ctx = self.create_context(Some(ctx_parent));
        let def = file.ir.component(comp);
        let mut objects: Vec<ObjectId> = Vec::with_capacity(def.objects.len());
        let mut inner: Vec<Option<Rc<Instance>>> = Vec::with_capacity(def.objects.len());
        for (i, od) in def.objects.iter().enumerate() {
            let meta = self.object_meta(&file, comp, i)?;
            let id = match (i, &root) {
                (0, RootSpec::Existing(id)) => *id,
                (0, RootSpec::New(owner)) => {
                    let id = self.create(meta, *owner)?;
                    self.set_context(id, ctx)?;
                    id
                }
                _ => {
                    let owner = od.parent.map(|p| objects[p.index()]);
                    let id = self.create(meta, owner)?;
                    self.set_context(id, ctx)?;
                    id
                }
            };
            if !matches!((i, &root), (0, RootSpec::Existing(_))) {
                self.set_location(id, &file.url, od.span.line, od.span.column);
                cr.all.push(id);
            }
            if let Some(name) = &od.id {
                self.set_context_id(ctx, name, id);
            }
            objects.push(id);
            inner.push(None);
            // A component file, or a module type that is written in QML
            // (`Engine::register_qml_type`).
            let component = match &od.ty {
                TypeRef::Component { file: f, inline } => Some((f.clone(), inline.clone())),
                TypeRef::Native { module, name } => {
                    self.qml_type_key(module, name).map(|k| (k, None))
                }
                TypeRef::Unresolved(name) => self.qml_type_key_any(name).map(|k| (k, None)),
            };
            if let Some((f, inline)) = &component {
                let inner_file = if *f == file.key {
                    file.clone()
                } else {
                    self.load_file(f)?
                };
                let inner_comp = inner_file
                    .find_component(inline.as_deref())
                    .ok_or_else(|| EngineError::Load(format!("{}: no component {f}", file.url)))?;
                inner[i] = Some(self.allocate(
                    cr,
                    inner_file,
                    inner_comp,
                    RootSpec::Existing(id),
                    None,
                    ctx,
                )?);
            }
        }
        if let Some(&root_id) = objects.first() {
            self.add_context(root_id, ctx);
        }
        let inst = Rc::new(Instance {
            file: file.clone(),
            comp,
            objects,
            inner,
            outer,
            context: ctx,
        });
        for (i, od) in def.objects.iter().enumerate() {
            let id = inst.objects[i];
            let node = cr.nodes.entry(id).or_default();
            for at in &od.attached {
                if is_component_attached(&at.ty) {
                    for h in &at.members.handlers {
                        if h.signal == "completed" {
                            node.handlers.push((inst.clone(), h.expr));
                        } else if h.signal == "reloaded" {
                            node.reloaded.push((inst.clone(), h.expr));
                        }
                    }
                }
            }
            if let Some(p) = od.parent {
                let parent = inst.objects[p.index()];
                cr.nodes.entry(parent).or_default().children.push(id);
            }
            cr.units.push(Unit {
                inst: inst.clone(),
                obj: i,
            });
        }
        cr.depth -= 1;
        Ok(inst)
    }

    /// The type of object `obj` of `comp`: its declared type, or a type
    /// made for it when it declares members or roots a component.
    pub(crate) fn object_meta(
        &self,
        file: &Rc<LoadedFile>,
        comp: CompIdx,
        obj: usize,
    ) -> Result<MetaRef, EngineError> {
        if let Some(m) = file.metas.borrow().get(&(comp.0, obj as u32)) {
            return Ok(m.clone());
        }
        let def = file.ir.component(comp);
        let od = &def.objects[obj];
        let base = self.type_meta(file, &od.ty)?;
        let roots = obj == 0 && matches!(def.kind, ComponentKind::File | ComponentKind::Inline);
        let declares = !od.properties.is_empty()
            || !od.signals.is_empty()
            || !od.functions.is_empty()
            || !od.enums.is_empty()
            || !od.required.is_empty();
        let meta = if declares || roots {
            let name = if roots {
                def.name.clone()
            } else {
                base.name.to_string()
            };
            let own_enums: Vec<&str> = od.enums.iter().map(|e| e.name.as_str()).collect();
            let mut b = MetaBuilder::new(&name).base(base);
            for e in &od.enums {
                let keys: Vec<(&str, i64)> = e.keys.iter().map(|(k, v)| (k.as_str(), *v)).collect();
                b = b.enumeration(&e.name, &keys);
            }
            for p in &od.properties {
                let mut flags = PropertyFlags::empty();
                if p.is_readonly {
                    flags |= PropertyFlags::READONLY;
                }
                if p.is_required {
                    flags |= PropertyFlags::REQUIRED;
                }
                let ty = if p.alias.is_some() {
                    flags |= PropertyFlags::ALIAS;
                    TypeTag::Var
                } else if p.is_list {
                    flags |= PropertyFlags::LIST;
                    let inner =
                        p.ty.strip_prefix("list<")
                            .and_then(|r| r.strip_suffix('>'))
                            .unwrap_or(&p.ty);
                    TypeTag::List(Some(Atom::intern(inner)))
                } else {
                    property_type(&p.ty, &own_enums)
                };
                if p.is_default {
                    flags |= PropertyFlags::DEFAULT;
                }
                b = b.property(&p.name, ty, flags, true);
                if p.is_default {
                    b = b.default_property(&p.name);
                }
            }
            for name in &od.required {
                b = b.require(name);
            }
            for s in &od.signals {
                let params = s
                    .params
                    .iter()
                    .map(|p| {
                        (
                            Atom::new(&p.name),
                            p.ty.as_deref()
                                .map_or(TypeTag::Var, |t| property_type(t, &own_enums)),
                        )
                    })
                    .collect();
                b = b.signal(&s.name, params);
            }
            for f in &od.functions {
                let params = file
                    .ir
                    .expr(f.expr)
                    .args
                    .iter()
                    .map(|a| (Atom::new(a), TypeTag::Var))
                    .collect();
                b = b.method(&f.name, params, TypeTag::Var, None);
            }
            b.build_shared()
        } else {
            base
        };
        file.metas
            .borrow_mut()
            .insert((comp.0, obj as u32), meta.clone());
        Ok(meta)
    }

    /// The type a [`TypeRef`] names, seen from `file`.
    fn type_meta(&self, file: &Rc<LoadedFile>, ty: &TypeRef) -> Result<MetaRef, EngineError> {
        match ty {
            TypeRef::Native { module, name } => self.native_meta(module, name),
            TypeRef::Component { file: f, inline } => {
                let lf = if *f == file.key {
                    file.clone()
                } else {
                    self.load_file(f)?
                };
                let c = lf
                    .find_component(inline.as_deref())
                    .ok_or_else(|| EngineError::Load(format!("{}: no component {f}", file.url)))?;
                self.object_meta(&lf, c, 0)
            }
            TypeRef::Unresolved(name) => {
                self.lookup_type_any(name)
                    .ok_or_else(|| EngineError::UnknownType {
                        uri: String::new(),
                        name: name.clone(),
                    })
            }
        }
    }

    /// `QtQuick` sees `QtQml`; the compiler's `QML` pseudo-module is
    /// `QtQml`.
    fn native_meta(&self, module: &str, name: &str) -> Result<MetaRef, EngineError> {
        let module = if module == "QML" { "QtQml" } else { module };
        self.lookup_type(module, name)
            .or_else(|| self.lookup_type("QtQml", name))
            .or_else(|| self.lookup_type_any(name))
            .ok_or_else(|| EngineError::UnknownType {
                uri: module.to_owned(),
                name: name.to_owned(),
            })
    }

    /// What the type `name` of `module` attaches to `obj`, made on first
    /// use: `other.Layout` in a script.
    pub(crate) fn attached_by_name(
        &self,
        obj: ObjectId,
        module: &str,
        name: &str,
    ) -> Option<ObjectId> {
        let meta = self.native_meta(module, name).ok()?;
        // A type written in QML attaches what the type it is made from does.
        meta.chain().find_map(|m| m.attached)?;
        self.attached_object(obj, &meta).ok()
    }

    // --------------------------------------------------------------- apply

    fn apply_aliases(&self, cr: &Creation) {
        for u in &cr.units {
            let od = &u.inst.file.ir.component(u.inst.comp).objects[u.obj];
            let id = u.inst.objects[u.obj];
            for p in &od.properties {
                let Some(alias) = &p.alias else { continue };
                let Some(target) = alias.object.and_then(|r| u.inst.resolve(r)) else {
                    self.warn(format!(
                        "{}:{}:{}: Invalid alias reference. Unable to find id \"{}\"",
                        u.inst.url(),
                        p.span.line,
                        p.span.column,
                        alias.id
                    ));
                    continue;
                };
                let Ok(me) = self.property_index(id, &p.name) else {
                    continue;
                };
                let r = if alias.path.is_empty() {
                    self.set_alias(id, me, target, None)
                } else {
                    match self.member_target(target, &alias.path[..alias.path.len() - 1], {
                        alias.path.last().expect("not empty")
                    }) {
                        Ok(t) => self.set_alias(id, me, t.obj, Some(t.prop)),
                        Err(e) => Err(e),
                    }
                };
                if let Err(e) = r {
                    self.warn(format!(
                        "{}:{}:{}: Invalid alias target location: {e}",
                        u.inst.url(),
                        p.span.line,
                        p.span.column
                    ));
                }
            }
        }
    }

    fn apply_unit(&self, cr: &Creation, i: usize) {
        let u = &cr.units[i];
        let inst = &u.inst;
        let od = &inst.file.ir.component(inst.comp).objects[u.obj];
        let id = inst.objects[u.obj];
        let list_model = match &od.ty {
            // Data for its ListModel, read from the IR below: what it is
            // given are role values, not properties.
            TypeRef::Native { name, .. } if name == "ListElement" => return,
            TypeRef::Native { name, .. } if name == "ListModel" => {
                // Before the members, so `onCountChanged` does not hear
                // of the rows the model is declared with.
                let rows = self.list_element_rows(inst, id, &od.children);
                crate::listmodel::declare_rows(self, id, rows);
                true
            }
            _ => false,
        };
        self.apply_members(inst, id, &od.members, &[], id);
        for g in &od.grouped {
            if !self.apply_value_members(inst, id, &g.members, &g.path) {
                self.apply_members(inst, id, &g.members, &g.path, id);
            }
        }
        for v in &od.children {
            match v {
                ObjectValue::Object(_) if list_model => {}
                ObjectValue::Object(j) => {
                    let child = inst.objects[j.index()];
                    let child_def = &inst.file.ir.component(inst.comp).objects[j.index()];
                    if let Some(on) = &child_def.on {
                        self.apply_value_source(id, child, on);
                        continue;
                    }
                    if let Err(e) = self.list_append_default(id, child) {
                        self.warn(format!(
                            "{}Cannot assign to non-existent default property ({e})",
                            self.location_of(child).unwrap_or_default()
                        ));
                    }
                }
                ObjectValue::Component(c) => {
                    let t = Template {
                        file: inst.file.clone(),
                        comp: *c,
                        outer: Some(inst.clone()),
                    };
                    if self
                        .meta_of(id)
                        .is_some_and(|m| m.inherits(<Component as QmlType>::static_meta()))
                    {
                        self.set_template(id, t);
                    } else if let Ok(c) = self.make_component_object(Some(id), t) {
                        let _ = self.list_append_default(id, c);
                    }
                }
            }
        }
        for f in &od.functions {
            let Ok(m) = self.method_index(id, &f.name) else {
                continue;
            };
            let noted = tier1::Noted::of(inst, f.expr);
            let inst = inst.clone();
            let expr = f.expr;
            let _ = self.set_script_method(
                id,
                m,
                Rc::new(move |e, _obj, args| Ok(e.call_expr(&inst, expr, args))),
            );
            self.tier1_note_method(id, m, noted);
        }
        for at in &od.attached {
            self.apply_attached(inst, id, at);
        }
    }

    /// `Type on property { }`: `source` is not a value of the property but
    /// operates on it (an animation, a Behavior). It stays owned by `obj`
    /// and goes to whoever registered for its kind.
    fn apply_value_source(&self, obj: ObjectId, source: ObjectId, on: &[String]) {
        let Some((name, path)) = on.split_last() else {
            return;
        };
        let at = self.location_of(source).unwrap_or_default();
        match self.member_target(obj, path, name) {
            Ok(t) => {
                if !self.offer_value_source(source, t.obj, t.prop) {
                    let ty = self.meta_of(source).map(|m| m.name.to_string());
                    self.warn(format!(
                        "{at}\"{}\" cannot operate on \"{}\"",
                        ty.unwrap_or_default(),
                        on.join(".")
                    ));
                }
            }
            Err(_) => self.warn(format!(
                "{at}Cannot assign to non-existent property \"{}\"",
                on.join(".")
            )),
        }
    }

    /// The rows the `ListElement`s among `values` declare, as role names
    /// and values. A list of elements given to a role is a model of its
    /// own, owned by `model`.
    fn list_element_rows(
        &self,
        inst: &Rc<Instance>,
        model: ObjectId,
        values: &[ObjectValue],
    ) -> Vec<Vec<(String, Value)>> {
        let comp = inst.file.ir.component(inst.comp);
        let mut rows = Vec::new();
        for v in values {
            let ObjectValue::Object(j) = v else { continue };
            let ed = &comp.objects[j.index()];
            if !matches!(&ed.ty, TypeRef::Native { name, .. } if name == "ListElement") {
                continue;
            }
            let mut row = Vec::new();
            for a in &ed.members.assignments {
                row.push((a.name.clone(), self.literal_value(inst, &a.value)));
            }
            // `cost: -1` is an expression to the compiler. Qt allows only
            // constants here, so once is enough.
            for b in &ed.members.bindings {
                if let Some(v) = self.eval_expr(inst, b.expr, &TypeTag::Var, &[]) {
                    row.push((b.name.clone(), v));
                }
            }
            for oa in &ed.members.objects {
                let nested = self.list_element_rows(inst, model, &oa.values);
                row.push((
                    oa.name.clone(),
                    crate::listmodel::nested_model(self, model, nested),
                ));
            }
            rows.push(row);
        }
        rows
    }

    fn apply_attached(&self, inst: &Rc<Instance>, id: ObjectId, at: &Attached) {
        if is_component_attached(&at.ty) {
            // `onCompleted` runs from the completion pass.
            for h in &at.members.handlers {
                if h.signal != "destruction" {
                    continue;
                }
                let Ok(att) = self
                    .attached_object(id, &MetaRef::Static(<Component as QmlType>::static_meta()))
                else {
                    continue;
                };
                self.inner.destruction_handlers.set(true);
                self.connect_handler(inst, att, "destruction", h.expr, id);
            }
            return;
        }
        let att = self
            .type_meta(&inst.file, &at.ty)
            .and_then(|m| self.attached_object(id, &m));
        match att {
            Ok(att) => self.apply_members(inst, att, &at.members, &[], id),
            Err(e) => self.warn(format!(
                "{}{}: {e}",
                self.location_of(id).unwrap_or_default(),
                at.written
            )),
        }
    }

    /// Applies `m` to `obj` (under the group `path`); handlers die with
    /// `scope`.
    fn apply_members(
        &self,
        inst: &Rc<Instance>,
        obj: ObjectId,
        m: &Members,
        path: &[String],
        scope: ObjectId,
    ) {
        for a in &m.assignments {
            let t = match self.member_target(obj, path, &a.name) {
                Ok(t) => t,
                Err(_) => {
                    self.no_property(inst, a.span.line, a.span.column, path, &a.name);
                    continue;
                }
            };
            self.apply_literal(inst, &t, &a.value);
        }
        for oa in &m.objects {
            let t = match self.member_target(obj, path, &oa.name) {
                Ok(t) => t,
                Err(_) => {
                    let at = self.location_of(obj);
                    self.warn(format!(
                        "{}Cannot assign to non-existent property \"{}\"",
                        at.unwrap_or_default(),
                        oa.name
                    ));
                    continue;
                }
            };
            let is_list = oa.is_list
                || self
                    .meta_of(t.obj)
                    .and_then(|mm| mm.property(t.prop).map(|p| p.is_list()))
                    .unwrap_or(false);
            for v in &oa.values {
                let value = match v {
                    ObjectValue::Object(j) => Some(inst.objects[j.index()]),
                    ObjectValue::Component(c) => self
                        .make_component_object(
                            Some(t.obj),
                            Template {
                                file: inst.file.clone(),
                                comp: *c,
                                outer: Some(inst.clone()),
                            },
                        )
                        .ok(),
                };
                let Some(value) = value else { continue };
                let r = if is_list {
                    self.list_append(t.obj, t.prop, value)
                } else {
                    self.write_internal(t.obj, t.prop, Value::Object(value), WriteKind::Initial)
                };
                if let Err(e) = r {
                    self.warn(format!(
                        "{}{e}",
                        self.location_of(value).unwrap_or_default()
                    ));
                }
            }
        }
        for h in &m.handlers {
            if path.is_empty() {
                self.connect_handler(inst, obj, &h.signal, h.expr, scope);
            } else {
                let b = inst.file.ir.expr(h.expr);
                self.no_property(
                    inst,
                    b.span.line,
                    b.span.column,
                    path,
                    &crate::qtqml::handler_name(&h.signal),
                );
            }
        }
        for pb in &m.bindings {
            let t = match self.member_target(obj, path, &pb.name) {
                Ok(t) => t,
                Err(_) => {
                    let b = inst.file.ir.expr(pb.expr);
                    self.no_property(inst, b.span.line, b.span.column, path, &pb.name);
                    continue;
                }
            };
            self.install_binding(inst, &t, pb);
        }
    }

    /// `font.pixelSize: 24; font.bold: flag`: members of a value-type
    /// property (`font`, `point`, `size`, `rect`) rather than of a group.
    /// Literals are folded into the value; with any binding, one binding
    /// on the whole property recomputes every bound member. Returns false
    /// when `path` is not a value-type property.
    fn apply_value_members(
        &self,
        inst: &Rc<Instance>,
        obj: ObjectId,
        m: &Members,
        path: &[String],
    ) -> bool {
        let [name] = path else { return false };
        let Ok(prop) = self.property_index(obj, name) else {
            return false;
        };
        let Some(ty) = self.meta_of(obj).and_then(|mm| {
            mm.property(prop)
                .filter(|p| !p.flags.contains(PropertyFlags::GROUP))
                .map(|p| p.ty.clone())
        }) else {
            return false;
        };
        if !matches!(
            ty,
            TypeTag::Font
                | TypeTag::Point
                | TypeTag::Size
                | TypeTag::Rect
                | TypeTag::Vector2d
                | TypeTag::Vector3d
                | TypeTag::Vector4d
                | TypeTag::Quaternion
        ) {
            return false;
        }
        let mut base = self.untrack(|| self.read(obj, prop));
        for a in &m.assignments {
            let v = self.literal_value(inst, &a.value);
            match base.with_member(&ty, &a.name, &v) {
                Some(b) => base = b,
                None => self.no_property(inst, a.span.line, a.span.column, path, &a.name),
            }
        }
        let mut bound = Vec::new();
        let mut first: Option<&PropertyBinding> = None;
        for pb in &m.bindings {
            if base.with_member(&ty, &pb.name, &Value::Undefined).is_some() {
                bound.push((pb.name.clone(), pb.expr));
                first.get_or_insert(pb);
            } else {
                let b = inst.file.ir.expr(pb.expr);
                self.no_property(inst, b.span.line, b.span.column, path, &pb.name);
            }
        }
        for oa in &m.objects {
            self.warn(format!(
                "{}Cannot assign to non-existent property \"{name}.{}\"",
                self.location_of(obj).unwrap_or_default(),
                oa.name
            ));
        }
        for h in &m.handlers {
            let b = inst.file.ir.expr(h.expr);
            self.no_property(
                inst,
                b.span.line,
                b.span.column,
                path,
                &crate::qtqml::handler_name(&h.signal),
            );
        }
        if bound.is_empty() {
            if let Err(e) = self.write_internal(obj, prop, base, WriteKind::Initial) {
                let at = self.location_of(obj).unwrap_or_default();
                self.warn(format!("{at}{e}"));
            }
            return true;
        }
        let Some(first) = first else { return true };
        let expr_at = inst.file.ir.expr(first.expr).span;
        let i = inst.clone();
        let r = self.bind_fallible(obj, prop, move |e| {
            let mut v = base.clone();
            for (member, expr) in &bound {
                let mv = e.eval_expr(&i, *expr, &TypeTag::Var, &[])?;
                v = v.with_member(&ty, member, &mv)?;
            }
            Some(v)
        });
        match r {
            Ok(()) => {
                let (o, p) = self.redirect(obj, prop);
                let member = Atom::new(&format!("{name}.{}", first.name));
                self.set_binding_loc(o, p, inst, name_place(first, expr_at), Some(member));
            }
            Err(e) => self.warn(format!(
                "{}:{}:{}: {e}",
                inst.url(),
                expr_at.line,
                expr_at.column
            )),
        }
        true
    }

    fn no_property(&self, inst: &Instance, line: u32, col: u32, path: &[String], name: &str) {
        let full = if path.is_empty() {
            name.to_owned()
        } else {
            format!("{}.{name}", path.join("."))
        };
        self.warn(format!(
            "{}:{line}:{col}: Cannot assign to non-existent property \"{full}\"",
            inst.url()
        ));
    }

    /// The property `path.name` of `obj`: a dotted group leaf
    /// (`border.width`), or a property of the object a path leads to.
    fn member_target(
        &self,
        obj: ObjectId,
        path: &[String],
        name: &str,
    ) -> Result<Target, EngineError> {
        if path.is_empty() {
            return match (self.property_index(obj, name), name.rsplit_once('.')) {
                (Ok(prop), _) => Ok(Target { obj, prop }),
                // What is attached is written with its whole path, which
                // may lead through an object: `ScrollBar.vertical.policy`.
                (Err(_), Some((through, leaf))) => {
                    let through: Vec<String> = through.split('.').map(str::to_owned).collect();
                    self.member_target(obj, &through, leaf)
                }
                (Err(e), None) => Err(e),
            };
        }
        let dotted = format!("{}.{name}", path.join("."));
        if let Ok(prop) = self.property_index(obj, &dotted) {
            return Ok(Target { obj, prop });
        }
        let mut cur = obj;
        for seg in path {
            cur = self
                .untrack(|| self.read_by_name(cur, seg))
                .as_object()
                .ok_or_else(|| EngineError::UnknownProperty {
                    ty: self
                        .meta_of(cur)
                        .map(|m| m.name.to_string())
                        .unwrap_or_default(),
                    name: dotted.clone(),
                })?;
        }
        let prop = self.property_index(cur, name)?;
        Ok(Target { obj: cur, prop })
    }

    fn apply_literal(&self, inst: &Rc<Instance>, t: &Target, lit: &Literal) {
        if let Literal::Translatable(tr) = lit
            && tr.kind != TranslationKind::NoOp
        {
            // A binding, so that it runs again when the language changes.
            let tr = tr.clone();
            let r = self.bind_fallible(t.obj, t.prop, move |e| {
                Some(Value::String(Atom::new(&e.translate_literal(&tr))))
            });
            if let Err(e) = r {
                self.warn(e.to_string());
            }
            return;
        }
        let v = self.literal_value(inst, lit);
        if let Err(e) = self.write_internal(t.obj, t.prop, v, WriteKind::Initial) {
            let at = self.location_of(t.obj).unwrap_or_default();
            self.warn(format!("{at}{e}"));
        }
    }

    fn literal_value(&self, inst: &Rc<Instance>, lit: &Literal) -> Value {
        match lit {
            Literal::Undefined => Value::Undefined,
            Literal::Null => Value::Null,
            Literal::Bool(b) => Value::Bool(*b),
            Literal::Number(n) => Value::Real(*n),
            Literal::String(s) => Value::String(Atom::new(s)),
            Literal::Color(s) => {
                Color::parse(s).map_or_else(|| Value::String(Atom::new(s)), Value::Color)
            }
            Literal::Enum(e) => {
                let v = e.value.or_else(|| {
                    let meta = self.type_meta(&inst.file, &e.ty).ok();
                    // `Qt` is a namespace, not a registered type.
                    let of_qt = matches!(&e.ty, TypeRef::Native { name, .. } if name == "Qt");
                    let named = |key: &str| {
                        meta.as_ref()
                            .and_then(|m| m.enum_key(key))
                            .or_else(|| of_qt.then(|| crate::js::qt_enum_key(key)).flatten())
                    };
                    // A string written for a flags property names several
                    // keys: "LeftButton|RightButton".
                    e.key
                        .split('|')
                        .map(named)
                        .try_fold(0, |all, one| Some(all | one?))
                });
                Value::Int(v.and_then(|v| i32::try_from(v).ok()).unwrap_or(0))
            }
            Literal::Translatable(t) => Value::String(Atom::new(&t.source)),
            Literal::List(items) => {
                Value::list(items.iter().map(|i| self.literal_value(inst, i)).collect())
            }
            Literal::Object(_) => self
                .run_js("object literal", |s| {
                    let v = literal_js(lit);
                    Ok(self.js_to_value(s, v, &TypeTag::Var))
                })
                .unwrap_or(Value::Undefined),
        }
    }

    fn connect_handler(
        &self,
        inst: &Rc<Instance>,
        obj: ObjectId,
        signal: &str,
        expr: ExprId,
        scope: ObjectId,
    ) {
        let i = inst.clone();
        let handler: crate::Handler = Rc::new(move |e: &Engine, args: &[Value]| {
            // Dev reload: the tree being replaced, or the one being
            // made, is not to act on what the other does.
            if e.reload_muted(scope) {
                return;
            }
            e.call_expr(&i, expr, args);
        });
        match self.signal_index(obj, signal) {
            Ok(s) => {
                let h = handler.clone();
                if let Err(e) = self.connect_to(obj, s, scope, move |e, a| h(e, a)) {
                    self.warn(e.to_string());
                }
            }
            Err(_)
                if self
                    .meta_of(obj)
                    .is_some_and(|m| m.inherits(<Connections as QmlType>::static_meta())) =>
            {
                if let Err(e) = self.connections_add(obj, signal, handler) {
                    self.warn(e.to_string());
                }
            }
            Err(_) => {
                let b = inst.file.ir.expr(expr);
                self.no_property(
                    inst,
                    b.span.line,
                    b.span.column,
                    &[],
                    &crate::qtqml::handler_name(signal),
                );
            }
        }
    }

    fn install_binding(&self, inst: &Rc<Instance>, t: &Target, pb: &PropertyBinding) {
        let expr = pb.expr;
        let b = inst.file.ir.expr(expr);
        let hint = self
            .meta_of(t.obj)
            .and_then(|m| m.property(t.prop).map(|p| p.ty.clone()))
            .unwrap_or(TypeTag::Var);
        let i = inst.clone();
        if self.is_script_property(t.obj, t.prop) {
            // A statement to run when asked, not a value to keep current.
            self.set_script(
                t.obj,
                t.prop,
                Rc::new(move |e| {
                    e.call_expr(&i, expr, &[]);
                }),
            );
            return;
        }
        let r = self.bind_fallible(t.obj, t.prop, move |e| e.eval_expr(&i, expr, &hint, &[]));
        match r {
            Ok(()) => {
                let (obj, prop) = self.redirect(t.obj, t.prop);
                self.set_binding_loc(obj, prop, inst, name_place(pb, b.span), None);
            }
            Err(e) => self.warn(format!(
                "{}:{}:{}: {e}",
                inst.url(),
                b.span.line,
                b.span.column
            )),
        }
    }

    // ---------------------------------------------------------- evaluation

    /// Runs a handler or function; its value, or `undefined` on error.
    pub(crate) fn call_expr(&self, inst: &Instance, expr: ExprId, args: &[Value]) -> Value {
        self.eval_expr(inst, expr, &TypeTag::Var, args)
            .unwrap_or(Value::Undefined)
    }

    /// Runs `expr`: in Rust when it is a property path, else its JS
    /// function. `None` when it threw (the warning is logged).
    fn eval_expr(
        &self,
        inst: &Instance,
        expr: ExprId,
        hint: &TypeTag,
        args: &[Value],
    ) -> Option<Value> {
        let file = &inst.file;
        let b = file.ir.expr(expr);
        if args.is_empty()
            && let Some(Some(t0)) = file.tier0.get(b.js_fn.index())
            && let Some(v) = self.tier0_eval(inst, b, t0)
        {
            return v;
        }
        if let Some(v) = self.tier1_eval(inst, expr, hint, args) {
            return v;
        }
        let f = match file.function(self, b.js_fn) {
            Ok(f) => f,
            Err(e) => {
                self.warn(e.to_string());
                return None;
            }
        };
        let this = inst
            .resolve(b.scope)
            .map_or(JsValue::Undefined, |o| self.to_js(&Value::Object(o)));
        self.run_js("binding", |s| {
            let mut a: Vec<JsValue> = Vec::with_capacity(b.params.len() + args.len());
            for p in &b.params {
                a.push(self.arg_js(s, inst, &p.arg));
            }
            a.extend(args.iter().map(|v| self.to_js(v)));
            let out = s.call_with_this(f, &this, &a)?;
            Ok(self.js_to_value(s, out, hint))
        })
    }

    /// A property path read in Rust. `None`: not expressible here, use JS.
    /// `Some(None)`: it failed as JS would (logged).
    fn tier0_eval(
        &self,
        inst: &Instance,
        b: &Binding,
        t0: &crate::loader::Tier0,
    ) -> Option<Option<Value>> {
        let mut cur = self.arg_value(inst, &b.params.get(t0.param)?.arg)?;
        let mut i = 0;
        let path = &t0.path;
        while i < path.len() {
            let name = &path[i];
            let o = match &cur {
                Value::Object(o) if self.is_alive(*o) => *o,
                Value::Null | Value::Undefined | Value::Object(_) => {
                    let what = if matches!(cur, Value::Undefined) {
                        "undefined"
                    } else {
                        "null"
                    };
                    self.warn(format!(
                        "{}:{}: TypeError: Cannot read property '{name}' of {what}",
                        inst.url(),
                        b.span.line
                    ));
                    return Some(None);
                }
                _ => return None,
            };
            match self.property_index(o, name) {
                Ok(p) => {
                    let group = self
                        .meta_of(o)
                        .and_then(|m| {
                            m.property(p)
                                .map(|i| i.flags.contains(PropertyFlags::GROUP))
                        })
                        .unwrap_or(false);
                    if group {
                        let leaf = path.get(i + 1)?;
                        let p = self.property_index(o, &format!("{name}.{leaf}")).ok()?;
                        cur = self.read(o, p);
                        i += 2;
                        continue;
                    }
                    cur = self.read(o, p);
                }
                Err(_) if name.as_str() == "parent" => {
                    cur = self.visual_parent(o).map_or(Value::Null, Value::Object);
                }
                Err(_) => return None,
            }
            i += 1;
        }
        Some(Some(cur))
    }

    /// A name the running program supplies. One tied to a component is
    /// a context property of that component's instance (a model role);
    /// one that is not may also be an id of whatever the instance was
    /// made in.
    fn context_arg(
        &self,
        inst: &Instance,
        component: Option<CompIdx>,
        name: &str,
    ) -> Option<Value> {
        match component {
            Some(c) => {
                let ctx = inst.find(c).map_or(inst.context, |i| i.context);
                self.context_property(ctx, name)
            }
            None => self.context_name(inst.context, name),
        }
    }

    /// A parameter as an engine value, for tier 0. `None` for what only JS
    /// can supply.
    fn arg_value(&self, inst: &Instance, arg: &Arg) -> Option<Value> {
        match arg {
            Arg::Object(r) => Some(inst.resolve(*r).map_or(Value::Null, Value::Object)),
            Arg::Property { object, name } => {
                let o = inst.resolve(*object)?;
                Some(self.read_by_name(o, name))
            }
            Arg::Context {
                component,
                name,
                fallback,
            } => match self.context_arg(inst, *component, name) {
                Some(v) => Some(v),
                None => self.arg_value(inst, fallback.as_deref()?),
            },
            Arg::Attached { ty, object } => {
                let o = inst.resolve(*object)?;
                let meta = self.type_meta(&inst.file, ty).ok()?;
                let attached = self.attached_object(o, &meta).ok()?;
                Some(Value::Object(attached))
            }
            _ => None,
        }
    }

    /// A parameter as JS sees it.
    fn arg_js(&self, s: &Scope<'_>, inst: &Instance, arg: &Arg) -> JsValue {
        match arg {
            Arg::Object(_) | Arg::Property { .. } | Arg::Attached { .. } => self
                .arg_value(inst, arg)
                .map_or(JsValue::Undefined, |v| self.to_js(&v)),
            // A singleton type has no instance yet, but its name still
            // reaches its enums (`Easing[name]`).
            Arg::Type(t) | Arg::Singleton(t) => match self.type_meta(&inst.file, t) {
                Ok(m) => JsValue::Map(
                    m.enums
                        .iter()
                        .flat_map(|e| e.keys.iter())
                        .map(|(k, v)| (k.to_string(), JsValue::Number(*v as f64)))
                        .collect(),
                ),
                Err(_) => JsValue::Undefined,
            },
            Arg::Namespace { .. } => JsValue::Undefined,
            Arg::Context {
                component,
                name,
                fallback,
            } => match self.context_arg(inst, *component, name) {
                Some(v) => self.to_js(&v),
                None => match fallback {
                    Some(f) => self.arg_js(s, inst, f),
                    None => global(s, name),
                },
            },
            Arg::Dynamic(name) => match self.context_name(inst.context, name) {
                Some(v) => self.to_js(&v),
                None => global(s, name),
            },
            Arg::Holder {
                component,
                name,
                fallback,
            } => {
                let ctx = match component {
                    Some(c) => inst.find(*c).map_or(inst.context, |i| i.context),
                    None => inst.context,
                };
                let held = |v: JsValue| JsValue::Map(vec![(name.clone(), v)]);
                match self.context_holder(ctx, name, component.is_none()) {
                    Some(Ok(o)) => self.to_js(&Value::Object(o)),
                    Some(Err(v)) => held(self.to_js(&v)),
                    None => match fallback.and_then(|r| inst.resolve(r)) {
                        Some(o) => self.to_js(&Value::Object(o)),
                        None => held(global(s, name)),
                    },
                }
            }
        }
    }

    // ---------------------------------------------------------- components

    pub(crate) fn make_component_object(
        &self,
        owner: Option<ObjectId>,
        template: Template,
    ) -> Result<ObjectId, EngineError> {
        let c = self.create(
            MetaRef::Static(<Component as QmlType>::static_meta()),
            owner,
        )?;
        self.set_template(c, template);
        Ok(c)
    }

    fn set_template(&self, c: ObjectId, template: Template) {
        let url = template.file.url.clone();
        let _ = self.with_mut::<Component, _>(c, |comp| {
            comp.template = Some(Rc::new(template));
            comp.url = url;
        });
        let _ = self.write_internal(
            c,
            Component::STATUS.index,
            Value::Int(1),
            WriteKind::Initial,
        );
    }
}

fn is_component_attached(ty: &TypeRef) -> bool {
    matches!(ty, TypeRef::Native { module, name } if name == "Component" && (module == "QML" || module == "QtQml"))
}

/// A QML property type: enum types are ints.
fn property_type(ty: &str, own_enums: &[&str]) -> TypeTag {
    if ty.contains('.') || own_enums.contains(&ty) {
        return TypeTag::Int;
    }
    TypeTag::parse(ty)
}

fn global(s: &Scope<'_>, name: &str) -> JsValue {
    s.globals()
        .get::<_, ljos_js::rquickjs::Value>(name)
        .ok()
        .and_then(|v| s.from_value(&v).ok())
        .unwrap_or(JsValue::Undefined)
}

/// An object literal (`{a: 1}`) as a JS value.
fn literal_js(lit: &Literal) -> JsValue {
    match lit {
        Literal::Undefined => JsValue::Undefined,
        Literal::Null => JsValue::Null,
        Literal::Bool(b) => JsValue::Bool(*b),
        Literal::Number(n) => JsValue::Number(*n),
        Literal::String(s) | Literal::Color(s) => JsValue::String(s.clone()),
        Literal::Enum(e) => JsValue::Number(e.value.unwrap_or(0) as f64),
        Literal::Translatable(t) => JsValue::String(t.source.clone()),
        Literal::List(items) => JsValue::Array(items.iter().map(literal_js).collect()),
        Literal::Object(fields) => JsValue::Map(
            fields
                .iter()
                .map(|(k, v)| (k.clone(), literal_js(v)))
                .collect(),
        ),
    }
}

/// Where a [`CreationJob`] is.
enum Phase {
    Load,
    Allocate(Rc<Template>),
    Apply(Creation, ObjectId, usize),
    Done,
}

#[derive(Default)]
struct HandleState {
    result: RefCell<Option<Result<ObjectId, EngineError>>>,
    cancelled: Cell<bool>,
    /// Applied objects over all objects.
    progress: Cell<f64>,
}

/// The result of a [`CreationJob`], readable once it is done, and the way
/// to call it off.
#[derive(Clone, Default)]
pub struct CreationHandle(Rc<HandleState>);

impl std::fmt::Debug for CreationHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("CreationHandle")
            .field(&*self.0.result.borrow())
            .finish()
    }
}

impl CreationHandle {
    pub fn is_done(&self) -> bool {
        self.0.result.borrow().is_some()
    }

    /// The root, or the error; `None` while still running.
    pub fn result(&self) -> Option<Result<ObjectId, EngineError>> {
        self.0.result.borrow().clone()
    }

    /// How much of the component is made, from 0 to 1.
    pub fn progress(&self) -> f64 {
        self.0.progress.get()
    }

    /// Calls the job off: at its next step it destroys what it made and
    /// ends with an error, without running its completion callback. Does
    /// nothing to a job that is done.
    pub fn cancel(&self) {
        self.0.cancelled.set(true);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.cancelled.get()
    }
}

/// What a [`CreationJob`] calls when it ends without being cancelled.
type DoneFn = Box<dyn FnOnce(&Engine, &Result<ObjectId, EngineError>)>;

/// See [`Engine::create_delegate`].
type DelegateSetup = Box<dyn FnOnce(&Engine, ObjectId, ContextId)>;

/// What a [`CreationJob`] makes its object from.
enum JobSource {
    Url(String),
    Component(ObjectId),
}

/// [`Engine::create_from_url`] as a resumable [`Job`]: loads, allocates,
/// then applies objects until the slice runs out, and finalizes. Unlike
/// the synchronous path each slice is its own batch, so a binding may see
/// a half-applied component in between (it re-runs once it is complete).
pub struct CreationJob {
    /// Weak, so a job waiting in [`Engine::incubate`]'s queue does not
    /// keep its engine alive.
    engine: Weak<crate::engine::Inner>,
    source: JobSource,
    parent: Option<ObjectId>,
    props: Vec<(String, Value)>,
    phase: Phase,
    lane: Lane,
    owner: ProcId,
    handle: CreationHandle,
    done: Option<DoneFn>,
    /// Set for a view's delegate, which is not a child of its owner.
    setup: Option<DelegateSetup>,
}

impl std::fmt::Debug for CreationJob {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut d = f.debug_struct("CreationJob");
        match &self.source {
            JobSource::Url(url) => d.field("url", url),
            JobSource::Component(c) => d.field("component", c),
        };
        d.field("parent", &self.parent)
            .field("lane", &self.lane)
            .finish_non_exhaustive()
    }
}

impl CreationJob {
    pub fn new(
        engine: &Engine,
        url: &str,
        parent: Option<ObjectId>,
        initial_props: Vec<(String, Value)>,
        lane: Lane,
        owner: ProcId,
    ) -> (Self, CreationHandle) {
        Self::with_source(
            engine,
            JobSource::Url(url.to_owned()),
            parent,
            initial_props,
            lane,
            owner,
        )
    }

    /// [`Engine::component_create`] as a job: makes an object from the
    /// `Component` object `component`.
    pub fn for_component(
        engine: &Engine,
        component: ObjectId,
        parent: Option<ObjectId>,
        initial_props: Vec<(String, Value)>,
        lane: Lane,
        owner: ProcId,
    ) -> (Self, CreationHandle) {
        Self::with_source(
            engine,
            JobSource::Component(component),
            parent,
            initial_props,
            lane,
            owner,
        )
    }

    fn with_source(
        engine: &Engine,
        source: JobSource,
        parent: Option<ObjectId>,
        props: Vec<(String, Value)>,
        lane: Lane,
        owner: ProcId,
    ) -> (Self, CreationHandle) {
        let handle = CreationHandle::default();
        (
            Self {
                engine: Rc::downgrade(&engine.inner),
                source,
                parent,
                props,
                phase: Phase::Load,
                lane,
                owner,
                handle: handle.clone(),
                done: None,
                setup: None,
            },
            handle,
        )
    }

    /// [`Engine::create_delegate`] as a job.
    pub(crate) fn for_delegate(
        engine: &Engine,
        component: ObjectId,
        owner: Option<ObjectId>,
        lane: Lane,
        proc: ProcId,
        setup: impl FnOnce(&Engine, ObjectId, ContextId) + 'static,
    ) -> (Self, CreationHandle) {
        let (mut job, handle) = Self::with_source(
            engine,
            JobSource::Component(component),
            owner,
            Vec::new(),
            lane,
            proc,
        );
        job.setup = Some(Box::new(setup));
        (job, handle)
    }

    /// Calls `f` with the result when the job ends, unless it was
    /// cancelled. Whoever waits for the object needs no polling.
    #[must_use]
    pub fn on_done(
        mut self,
        f: impl FnOnce(&Engine, &Result<ObjectId, EngineError>) + 'static,
    ) -> Self {
        self.done = Some(Box::new(f));
        self
    }

    fn finish(&mut self, engine: Option<&Engine>, r: Result<ObjectId, EngineError>) -> JobPoll {
        self.phase = Phase::Done;
        self.handle.0.progress.set(1.0);
        *self.handle.0.result.borrow_mut() = Some(r.clone());
        if let (Some(e), Some(f)) = (engine, self.done.take())
            && !self.handle.is_cancelled()
        {
            f(e, &r);
        }
        JobPoll::Done
    }

    /// Ends a job nobody waits for any more: what it made goes away.
    fn abandon(&mut self, engine: Option<&Engine>, why: EngineError) -> JobPoll {
        if let (Some(e), Phase::Apply(_, root, _)) = (engine, &self.phase) {
            e.destroy_now(*root);
        }
        // Not `finish`: the callback belongs to whoever called it off.
        self.done = None;
        self.finish(None, Err(why))
    }

    /// Runs one step; `check` says whether the slice is spent.
    pub fn step(&mut self, check: &dyn Fn() -> bool) -> JobPoll {
        if matches!(self.phase, Phase::Done) {
            return JobPoll::Done;
        }
        let Some(e) = self.engine.upgrade().map(Engine::from_inner) else {
            return self.abandon(None, EngineError::Load("the engine is gone".into()));
        };
        loop {
            if self.handle.is_cancelled() {
                return self.abandon(Some(&e), EngineError::Load("creation was cancelled".into()));
            }
            if let Some(p) = self.parent.filter(|p| !e.is_alive(*p)) {
                return self.abandon(Some(&e), EngineError::DeadObject(p));
            }
            match std::mem::replace(&mut self.phase, Phase::Done) {
                Phase::Load => {
                    let template = match &self.source {
                        JobSource::Url(url) => e.file_template(url).map(Rc::new),
                        JobSource::Component(c) => e.component_template(*c),
                    };
                    match template {
                        Ok(t) => self.phase = Phase::Allocate(t),
                        Err(err) => return self.finish(Some(&e), Err(err)),
                    }
                }
                Phase::Allocate(t) => {
                    let mut cr = Creation::default();
                    let setup = self.setup.take();
                    match e.allocate_template_as(&mut cr, &t, self.parent, setup.is_none()) {
                        Ok(root) => {
                            if let (Some(setup), Some(top)) = (setup, &cr.top) {
                                setup(&e, root, top.context);
                            }
                            e.reload_assign(&cr, root);
                            e.apply_aliases(&cr);
                            self.phase = Phase::Apply(cr, root, 0);
                        }
                        Err(err) => return self.finish(Some(&e), Err(err)),
                    }
                }
                Phase::Apply(mut cr, root, mut next) => {
                    if !e.is_alive(root) {
                        return self.finish(Some(&e), Err(EngineError::DeadObject(root)));
                    }
                    let ((), held) = e.hold_eager(|| {
                        e.batch(|| {
                            while next < cr.units.len() {
                                e.apply_unit(&cr, next);
                                next += 1;
                                if check() {
                                    break;
                                }
                            }
                        })
                    });
                    cr.held.extend(held);
                    if next < cr.units.len() {
                        self.handle
                            .0
                            .progress
                            .set(next as f64 / cr.units.len() as f64);
                        self.phase = Phase::Apply(cr, root, next);
                        return JobPoll::Yield;
                    }
                    let props = std::mem::take(&mut self.props);
                    let r = e.batch(|| {
                        let r = e.finish_props(root, &props);
                        e.run_held(&cr);
                        r
                    });
                    if r.is_ok() {
                        e.release_held(&cr);
                    }
                    let r = e.finalize(cr, root, r);
                    return self.finish(Some(&e), r);
                }
                Phase::Done => return JobPoll::Done,
            }
            if check() {
                return JobPoll::Yield;
            }
        }
    }
}

impl<C> Job<C> for CreationJob {
    fn lane(&self) -> Lane {
        self.lane
    }

    fn owner(&self) -> ProcId {
        self.owner
    }

    fn poll(&mut self, cx: &mut JobCx<'_, C>) -> JobPoll {
        let budget = cx.budget;
        self.step(&|| budget.check())
    }
}

impl Engine {
    /// Hands `job` to whoever runs the frame (a scene's incubation step):
    /// it waits here until [`Engine::take_incubations`], and the host is
    /// woken as for a timer.
    pub fn incubate(&self, job: CreationJob) {
        self.inner.loader.borrow_mut().incubating.push(job);
        self.wake_host();
    }

    /// The jobs queued by [`Engine::incubate`], for a
    /// [`ljos_runtime::Scheduler`].
    pub fn take_incubations(&self) -> Vec<CreationJob> {
        std::mem::take(&mut self.inner.loader.borrow_mut().incubating)
    }

    pub fn has_incubations(&self) -> bool {
        !self.inner.loader.borrow().incubating.is_empty()
    }
}
