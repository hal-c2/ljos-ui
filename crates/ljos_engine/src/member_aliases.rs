//! An alias of one member of a property whose value has members:
//! `property alias family: label.font.family`.
//!
//! The alias redirects to the whole property (`font`), so it is watched
//! there and its change signal is the property's. What is written to it is
//! the whole value with that member changed. A binding on it gives that
//! member only: every member an alias binds is one binding of the whole
//! property, so that two aliases of one `font` both stay bound, and a
//! write through one of them ends its own binding only, as in Qt.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::WriteKind;
use crate::atom::Atom;
use crate::engine::{BindingFn, Engine, Free};
use crate::error::EngineError;
use crate::ids::{ObjectId, PropIndex};
use crate::object::AliasTarget;
use crate::value::Value;

type Members = Rc<RefCell<Vec<(Atom, BindingFn)>>>;

/// The members of each property that aliases bind, with their bindings.
#[derive(Default)]
struct MemberBindings {
    of: RefCell<HashMap<(ObjectId, PropIndex), Members>>,
}

impl Engine {
    /// The property and member an alias of a member leads to, through any
    /// aliases before it.
    pub(crate) fn alias_member(
        &self,
        obj: ObjectId,
        prop: PropIndex,
    ) -> Option<(ObjectId, PropIndex, Atom)> {
        let mut cur = (obj, prop);
        // Bounded as `redirect` is.
        for _ in 0..32 {
            let target = {
                let store = self.inner.store.borrow();
                let h = store.objects.get(cur.0)?;
                if !h.meta.property(cur.1)?.is_alias() {
                    return None;
                }
                h.alias(cur.1)?
            };
            match target {
                AliasTarget {
                    object,
                    prop: Some(tp),
                    member: Some(m),
                } => return Some((object, tp, m)),
                AliasTarget {
                    object,
                    prop: Some(tp),
                    member: None,
                } => cur = (object, tp),
                AliasTarget { prop: None, .. } => return None,
            }
        }
        None
    }

    /// The value of `obj`'s `prop` as it is now with its member `member`
    /// set to `v`.
    fn with_member_of(
        &self,
        obj: ObjectId,
        prop: PropIndex,
        member: &str,
        v: &Value,
    ) -> Result<Value, EngineError> {
        let meta = self.meta_of(obj).ok_or(EngineError::DeadObject(obj))?;
        let p = meta
            .property(prop)
            .ok_or_else(|| EngineError::UnknownProperty {
                ty: meta.name.to_string(),
                name: member.to_owned(),
            })?;
        let whole = self.untrack(|| self.read(obj, prop));
        whole
            .with_member(&p.ty, member, v)
            .ok_or_else(|| EngineError::UnknownProperty {
                ty: meta.name.to_string(),
                name: format!("{}.{member}", p.name),
            })
    }

    /// A write through an alias of `to`'s `tp`'s `member`.
    pub(crate) fn write_member(
        &self,
        (to, tp, member): (ObjectId, PropIndex, Atom),
        v: &Value,
        how: WriteKind,
        internal: bool,
    ) -> Result<(), EngineError> {
        let whole = self.with_member_of(to, tp, &member, v)?;
        if !matches!(how, WriteKind::Imperative) {
            return self.write_impl(to, tp, whole, how, internal);
        }
        // A script's write ends the binding of this member, not of the
        // others; the write ends the whole binding, so it is made again.
        let others = self.bound_members(to, tp).filter(|list| {
            list.borrow_mut().retain(|(m, _)| *m != member);
            !list.borrow().is_empty()
        });
        self.write_impl(to, tp, whole, how, internal)?;
        match others {
            Some(list) => self.bind_members(to, tp, list, false, Free::Never),
            None => Ok(()),
        }
    }

    /// A binding through an alias of `to`'s `tp`'s `member`.
    pub(crate) fn bind_member(
        &self,
        (to, tp, member): (ObjectId, PropIndex, Atom),
        f: BindingFn,
        internal: bool,
        free: Free,
    ) -> Result<(), EngineError> {
        let all = self.extension::<MemberBindings>();
        let list = all.of.borrow_mut().entry((to, tp)).or_default().clone();
        // What was bound before a write of the whole property went with it.
        if !self.is_bound(to, tp) {
            list.borrow_mut().clear();
        }
        {
            let mut l = list.borrow_mut();
            l.retain(|(m, _)| *m != member);
            l.push((member, f));
        }
        self.bind_members(to, tp, list, internal, free)
    }

    fn bound_members(&self, to: ObjectId, tp: PropIndex) -> Option<Members> {
        if !self.is_bound(to, tp) {
            return None;
        }
        let all = self.extension::<MemberBindings>();
        all.of.borrow().get(&(to, tp)).cloned()
    }

    fn bind_members(
        &self,
        to: ObjectId,
        tp: PropIndex,
        list: Members,
        internal: bool,
        free: Free,
    ) -> Result<(), EngineError> {
        let ty = self
            .meta_of(to)
            .and_then(|m| m.property(tp).map(|p| p.ty.clone()))
            .ok_or(EngineError::DeadObject(to))?;
        let whole: BindingFn = Box::new(move |e| {
            let mut l = list.try_borrow_mut().ok()?;
            // The members no alias binds are as they are now.
            let mut v = e.untrack(|| e.read(to, tp));
            for (member, f) in l.iter_mut() {
                v = v.with_member(&ty, member, &f(e)?)?;
            }
            Some(v)
        });
        self.bind_free(to, tp, whole, internal, free)
    }
}
