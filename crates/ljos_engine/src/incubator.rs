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
//!    applied before the level that uses it.
//! 3. **Finalize**: initial properties, the required check, native
//!    completion hooks, then `Component.onCompleted` in Qt's order (each
//!    object before its children, children last first; a component's own
//!    handler before its user's).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use ljos_ir::{
    Arg, Attached, Binding, CompIdx, ComponentKind, ExprId, Literal, Members, ObjRef, ObjectValue,
    PropertyDecl, TranslationKind, TypeRef,
};
use ljos_js::{JsValue, Scope};
use ljos_runtime::{Job, JobCx, JobPoll, Lane, ProcId};

use crate::atom::Atom;
use crate::engine::Engine;
use crate::error::EngineError;
use crate::ids::{ContextId, ObjectId, PropIndex};
use crate::loader::LoadedFile;
use crate::meta::{MetaBuilder, MetaRef, PropertyFlags};
use crate::object::{Location, QmlType};
use crate::qtqml::{Component, Connections};
use crate::types::TypeTag;
use crate::value::{Color, Value};
use crate::{Provenance, WriteKind};

/// Deepest nesting of file components; deeper is a component using
/// itself.
const MAX_DEPTH: u32 = 64;

/// The objects one component instance is made of.
pub(crate) struct Instance {
    pub(crate) file: Rc<LoadedFile>,
    pub(crate) comp: CompIdx,
    pub(crate) objects: Vec<ObjectId>,
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
    children: Vec<ObjectId>,
}

#[derive(Default)]
struct Creation {
    units: Vec<Unit>,
    /// In allocation order.
    all: Vec<ObjectId>,
    nodes: HashMap<ObjectId, CompletionNode>,
    depth: u32,
}

enum RootSpec {
    New(Option<ObjectId>),
    /// The object that uses the component (`Foo {}`), already made.
    Existing(ObjectId),
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
        self.create_from_template(&template, parent, initial_props)
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
        let key = Engine::url_key(url);
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
        let r = self.batch(|| -> Result<(), EngineError> {
            self.apply_aliases(&cr);
            for i in 0..cr.units.len() {
                self.apply_unit(&cr, i);
            }
            self.finish_props(root, &props)
        });
        self.finalize(cr, root, r)
    }

    fn allocate_template(
        &self,
        cr: &mut Creation,
        template: &Template,
        parent: Option<ObjectId>,
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
        // The instance (and so its contexts) lives as long as its root.
        self.keep_with(root, inst.clone());
        if let Some(p) = parent
            && self.default_property(p).is_ok()
        {
            self.list_append_default(p, root)?;
        }
        Ok(root)
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
            self.destroy_now(root);
            return Err(e);
        }
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
            for (inst, expr) in &node.handlers {
                if self.is_alive(o) {
                    self.call_expr(inst, *expr, &[]);
                }
            }
            stack.extend(node.children.iter().copied());
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

    fn add_binding_loc(&self, obj: ObjectId, prop: PropIndex, at: (u32, u32), declared: bool) {
        if let Some(h) = self.inner.store.borrow_mut().objects.get_mut(obj) {
            h.ext_mut().binding_locs.push((prop, at.0, at.1, declared));
        }
    }

    fn add_context(&self, obj: ObjectId, ctx: ContextId) {
        if let Some(h) = self.inner.store.borrow_mut().objects.get_mut(obj) {
            h.ext_mut().contexts.push(ctx);
        }
    }

    /// `url:line:col: ` of an object made from QML.
    fn location_of(&self, obj: ObjectId) -> Option<String> {
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
            if let TypeRef::Component { file: f, inline } = &od.ty {
                let inner_file = if *f == file.key {
                    file.clone()
                } else {
                    self.load_file(f)?
                };
                let inner_comp = inner_file
                    .find_component(inline.as_deref())
                    .ok_or_else(|| EngineError::Load(format!("{}: no component {f}", file.url)))?;
                self.allocate(
                    cr,
                    inner_file,
                    inner_comp,
                    RootSpec::Existing(id),
                    None,
                    ctx,
                )?;
            }
        }
        if let Some(&root_id) = objects.first() {
            self.add_context(root_id, ctx);
        }
        let inst = Rc::new(Instance {
            file: file.clone(),
            comp,
            objects,
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
    fn object_meta(
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
        self.apply_members(inst, id, &od.members, &[], id, &od.properties);
        for g in &od.grouped {
            if !self.apply_value_members(inst, id, &g.members, &g.path) {
                self.apply_members(inst, id, &g.members, &g.path, id, &[]);
            }
        }
        for v in &od.children {
            match v {
                ObjectValue::Object(j) => {
                    let child = inst.objects[j.index()];
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
            let inst = inst.clone();
            let expr = f.expr;
            let _ = self.set_script_method(
                id,
                m,
                Rc::new(move |e, _obj, args| Ok(e.call_expr(&inst, expr, args))),
            );
        }
        for at in &od.attached {
            self.apply_attached(inst, id, at);
        }
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
                self.connect_handler(inst, att, "destruction", h.expr, id);
            }
            return;
        }
        let att = self
            .type_meta(&inst.file, &at.ty)
            .and_then(|m| self.attached_object(id, &m));
        match att {
            Ok(att) => self.apply_members(inst, att, &at.members, &[], id, &[]),
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
        decls: &[PropertyDecl],
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
            // Qt names a declared property by its declaration.
            let decl = decls
                .iter()
                .find(|d| d.name == pb.name)
                .map(|d| (d.span.line, d.span.column));
            self.install_binding(inst, &t, pb.expr, decl);
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
            TypeTag::Font | TypeTag::Point | TypeTag::Size | TypeTag::Rect
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
        for pb in &m.bindings {
            if base.with_member(&ty, &pb.name, &Value::Undefined).is_some() {
                bound.push((pb.name.clone(), pb.expr));
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
        let at = {
            let b = inst.file.ir.expr(bound[0].1);
            (b.span.line, b.span.column)
        };
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
                self.add_binding_loc(o, p, at, false);
            }
            Err(e) => self.warn(format!("{}:{}:{}: {e}", inst.url(), at.0, at.1)),
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
            let prop = self.property_index(obj, name)?;
            return Ok(Target { obj, prop });
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
            let text = Value::String(Atom::new(&tr.source));
            let r = self.bind_fallible(t.obj, t.prop, move |e| {
                e.track_translation();
                Some(text.clone())
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
                let v = e
                    .value
                    .or_else(|| {
                        self.type_meta(&inst.file, &e.ty)
                            .ok()
                            .and_then(|m| m.enum_key(&e.key))
                    })
                    .or_else(|| {
                        // `Qt` is a namespace, not a registered type.
                        matches!(&e.ty, TypeRef::Native { name, .. } if name == "Qt")
                            .then(|| crate::js::qt_enum_key(&e.key))
                            .flatten()
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

    fn install_binding(
        &self,
        inst: &Rc<Instance>,
        t: &Target,
        expr: ExprId,
        decl: Option<(u32, u32)>,
    ) {
        let b = inst.file.ir.expr(expr);
        let hint = self
            .meta_of(t.obj)
            .and_then(|m| m.property(t.prop).map(|p| p.ty.clone()))
            .unwrap_or(TypeTag::Var);
        let i = inst.clone();
        let r = self.bind_fallible(t.obj, t.prop, move |e| e.eval_expr(&i, expr, &hint, &[]));
        match r {
            Ok(()) => {
                let (obj, prop) = self.redirect(t.obj, t.prop);
                // Without a declaration this is the expression's start,
                // not the property name's (the IR has no name span).
                let at = decl.unwrap_or((b.span.line, b.span.column));
                self.add_binding_loc(obj, prop, at, decl.is_some());
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
            } => {
                let ctx = match component {
                    Some(c) => inst.find(*c).map_or(inst.context, |i| i.context),
                    None => inst.context,
                };
                match self.context_property(ctx, name) {
                    Some(v) => Some(v),
                    None => self.arg_value(inst, fallback.as_deref()?),
                }
            }
            _ => None,
        }
    }

    /// A parameter as JS sees it.
    fn arg_js(&self, s: &Scope<'_>, inst: &Instance, arg: &Arg) -> JsValue {
        match arg {
            Arg::Object(_) | Arg::Property { .. } => self
                .arg_value(inst, arg)
                .map_or(JsValue::Undefined, |v| self.to_js(&v)),
            Arg::Type(t) => match self.type_meta(&inst.file, t) {
                Ok(m) => JsValue::Map(
                    m.enums
                        .iter()
                        .flat_map(|e| e.keys.iter())
                        .map(|(k, v)| (k.to_string(), JsValue::Number(*v as f64)))
                        .collect(),
                ),
                Err(_) => JsValue::Undefined,
            },
            Arg::Singleton(_) | Arg::Namespace { .. } => JsValue::Undefined,
            Arg::Context {
                component,
                name,
                fallback,
            } => {
                let ctx = match component {
                    Some(c) => inst.find(*c).map_or(inst.context, |i| i.context),
                    None => inst.context,
                };
                match self.context_property(ctx, name) {
                    Some(v) => self.to_js(&v),
                    None => match fallback {
                        Some(f) => self.arg_js(s, inst, f),
                        None => global(s, name),
                    },
                }
            }
            Arg::Dynamic(name) => match self.context_property(inst.context, name) {
                Some(v) => self.to_js(&v),
                None => global(s, name),
            },
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
            },
            handle,
        )
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
                    match e.allocate_template(&mut cr, &t, self.parent) {
                        Ok(root) => {
                            e.apply_aliases(&cr);
                            self.phase = Phase::Apply(cr, root, 0);
                        }
                        Err(err) => return self.finish(Some(&e), Err(err)),
                    }
                }
                Phase::Apply(cr, root, mut next) => {
                    if !e.is_alive(root) {
                        return self.finish(Some(&e), Err(EngineError::DeadObject(root)));
                    }
                    e.batch(|| {
                        while next < cr.units.len() {
                            e.apply_unit(&cr, next);
                            next += 1;
                            if check() {
                                break;
                            }
                        }
                    });
                    if next < cr.units.len() {
                        self.handle
                            .0
                            .progress
                            .set(next as f64 / cr.units.len() as f64);
                        self.phase = Phase::Apply(cr, root, next);
                        return JobPoll::Yield;
                    }
                    let props = std::mem::take(&mut self.props);
                    let r = e.batch(|| e.finish_props(root, &props));
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
