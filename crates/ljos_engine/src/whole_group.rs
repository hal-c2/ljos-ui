//! A grouped property given or taken back whole: `palette: Palette {}`,
//! `palette: other.palette`, `x.palette = y.palette`, `x.palette = undefined`.
//!
//! A type says which of its groups may be ([`MetaBuilder::whole_group`]).
//! What such a group is given is what was given a value in the source,
//! member by member; a member given nothing there is taken back here, so
//! that it is what it would be had nothing been set on it, as on qml6. A
//! group given in QML follows its source; one assigned from a script is a
//! copy of it as it is then.
//!
//! Behaviour is pinned by `features/controls/palette-assign.feature`.
//!
//! [`MetaBuilder::whole_group`]: crate::MetaBuilder::whole_group

use std::cell::RefCell;
use std::collections::HashMap;

use ljos_reactive::{NodeId, Provenance, WriteKind};

use crate::engine::Engine;
use crate::error::EngineError;
use crate::ids::{ObjectId, PropIndex};
use crate::meta::{PropertyFlags, Reset};
use crate::types::ConvertError;
use crate::value::Value;

/// `group = undefined`: every member is taken back.
pub(crate) const RESET: Reset = Reset::Custom {
    apply: take_back,
    value: nothing,
};

fn take_back(engine: &Engine, obj: ObjectId, root: PropIndex) -> Result<(), EngineError> {
    engine.reset_group(obj, root)
}

/// A group holds no value of its own.
fn nothing(_: &Engine, _: ObjectId, _: PropIndex) -> Value {
    Value::Undefined
}

/// The groups given in QML that follow their source: what stops each
/// when the group is given anew or taken back.
#[derive(Default)]
struct Following {
    effects: RefCell<HashMap<(ObjectId, PropIndex), NodeId>>,
}

/// Whether the author gave the member a value: assigned or bound.
fn given(p: Option<Provenance>) -> bool {
    matches!(p, Some(Provenance::Assigned | Provenance::Bound))
}

/// Each member of the target, the source's member of the same name, and
/// for a member of a group inside a group the outer group's member of that
/// name in the source, which stands for it where it was given nothing: a
/// colour set on a palette is set in each of its colour groups.
type Pairs = Vec<(PropIndex, Option<PropIndex>, Option<PropIndex>)>;

impl Engine {
    /// Whether `prop` of `obj` is a group given and taken back whole.
    pub(crate) fn is_whole_group(&self, obj: ObjectId, prop: PropIndex) -> bool {
        let store = self.inner.store.borrow();
        store
            .objects
            .get(obj)
            .and_then(|h| h.meta.property(prop))
            .is_some_and(|p| p.flags.contains(PropertyFlags::GROUP) && p.reset.is_some())
    }

    /// The members of the group `root` of `obj`, the groups inside it
    /// left out (their members are among them), with the name each has
    /// after the group's; and the group's name.
    fn members(
        &self,
        obj: ObjectId,
        root: PropIndex,
    ) -> Option<(String, Vec<(PropIndex, String)>)> {
        let meta = self.meta_of(obj)?;
        let name = meta.property(root)?.name.to_string();
        let prefix = format!("{name}.");
        let members = meta
            .properties
            .iter()
            .enumerate()
            .filter(|(_, p)| !p.flags.contains(PropertyFlags::GROUP))
            .filter_map(|(i, p)| {
                let leaf = p.name.strip_prefix(prefix.as_str())?;
                Some((PropIndex(u32::try_from(i).ok()?), leaf.to_owned()))
            })
            .collect();
        Some((name, members))
    }

    /// Takes back every member of the group `root` of `obj` that was given
    /// a value, and stops it following a group it was given in QML.
    pub(crate) fn reset_group(&self, obj: ObjectId, root: PropIndex) -> Result<(), EngineError> {
        self.stop_following(obj, root);
        let Some((name, members)) = self.members(obj, root) else {
            return Ok(());
        };
        if let Some((outer, own)) = name.rsplit_once('.') {
            self.untrack(|| self.leave_inner_group(obj, outer, own, &members))?;
        }
        for (p, _) in members {
            if given(self.provenance(obj, p)) {
                self.reset_member(obj, p)?;
            }
        }
        Ok(())
    }

    /// The group `own` inside the group `outer` is taken back: a member
    /// given on the outer group stands for this group's member no more,
    /// and stays given in each other group inside it that has not one of
    /// its own. qml6 takes `palette.button` out of the active group alone
    /// when `palette.active` is taken back.
    fn leave_inner_group(
        &self,
        obj: ObjectId,
        outer: &str,
        own: &str,
        members: &[(PropIndex, String)],
    ) -> Result<(), EngineError> {
        let Some(meta) = self.meta_of(obj) else {
            return Ok(());
        };
        let inner = format!("{outer}.");
        let others: Vec<&str> = meta
            .properties
            .iter()
            .filter(|p| p.flags.contains(PropertyFlags::GROUP) && p.reset.is_some())
            .filter_map(|p| p.name.strip_prefix(inner.as_str()))
            .filter(|g| !g.contains('.') && *g != own)
            .collect();
        for (_, leaf) in members {
            let Ok(o) = self.property_index(obj, &format!("{outer}.{leaf}")) else {
                continue;
            };
            if !given(self.provenance(obj, o)) {
                continue;
            }
            let v = self.read(obj, o);
            for g in &others {
                if let Ok(p) = self.property_index(obj, &format!("{outer}.{g}.{leaf}"))
                    && !given(self.provenance(obj, p))
                {
                    self.write_impl(obj, p, v.clone(), WriteKind::Imperative, true)?;
                }
            }
            self.reset_member(obj, o)?;
        }
        Ok(())
    }

    fn reset_member(&self, obj: ObjectId, p: PropIndex) -> Result<(), EngineError> {
        if self.reset_of(obj, p).is_some() {
            self.reset_impl(obj, p, true)
        } else {
            self.reset_to_default(obj, p)
        }
    }

    fn stop_following(&self, obj: ObjectId, root: PropIndex) {
        let effect = self
            .extension::<Following>()
            .effects
            .borrow_mut()
            .remove(&(obj, root));
        if let Some(effect) = effect {
            self.stop_effect(effect);
        }
    }

    /// Gives the group `root` of `obj` what the group of `src` whose
    /// members are named `prefix` and then theirs was given (`prefix`
    /// empty: `src` is an object of the group's type). `follow` keeps it
    /// doing so whenever the source changes, as a group given in QML does.
    ///
    /// A group inside a group (`palette.active`) given another such group
    /// is given the source's group of its own name: qml6 gives
    /// `palette.disabled: a.palette.active` the disabled colours of `a`.
    pub(crate) fn assign_group(
        &self,
        obj: ObjectId,
        root: PropIndex,
        src: ObjectId,
        prefix: &str,
        follow: bool,
    ) -> Result<(), EngineError> {
        self.stop_following(obj, root);
        let Some((name, members)) = self.members(obj, root) else {
            return Ok(());
        };
        // An object of the group's type is the outer group itself.
        let whole_object = prefix.is_empty();
        let prefix = self.same_named_group(src, prefix, &name);
        let outer_of = |member: &str| {
            let (inner, leaf) = member.rsplit_once('.')?;
            if !(inner.contains('.') || whole_object) || !self.is_group_named(src, inner) {
                return None;
            }
            let outer = match inner.rsplit_once('.') {
                Some((parent, _)) => format!("{parent}.{leaf}"),
                None => leaf.to_owned(),
            };
            self.property_index(src, &outer).ok()
        };
        let pairs: Pairs = members
            .into_iter()
            .map(|(to, leaf)| {
                let member = format!("{prefix}{leaf}");
                // A whole palette copies the outer member as itself, so
                // only a group given on its own needs it to stand in.
                let outer = if leaf.contains('.') {
                    None
                } else {
                    outer_of(&member)
                };
                (to, self.property_index(src, &member).ok(), outer)
            })
            .collect();
        if pairs.iter().all(|(_, from, _)| from.is_none()) {
            let ty = self
                .meta_of(obj)
                .and_then(|m| m.property(root).map(|p| p.ty.name()))
                .unwrap_or_default();
            let from = self
                .meta_of(src)
                .map(|m| m.name.to_string())
                .unwrap_or_default();
            return Err(ConvertError::new(from, ty).into());
        }
        if !follow {
            return self.untrack(|| self.copy_members(obj, src, &pairs, false));
        }
        let effect = self.effect_in(obj, move |e| {
            if let Err(err) = e.copy_members(obj, src, &pairs, true) {
                e.warn(err.to_string());
            }
        })?;
        self.extension::<Following>()
            .effects
            .borrow_mut()
            .insert((obj, root), effect);
        Ok(())
    }

    /// `prefix` of a group inside a group of `src`, as the group of that
    /// same group named as the last part of `target`, where there is one.
    fn same_named_group(&self, src: ObjectId, prefix: &str, target: &str) -> String {
        let group = prefix.trim_end_matches('.');
        let (Some((_, own)), false) = (target.rsplit_once('.'), group.is_empty()) else {
            return prefix.to_owned();
        };
        let parent = group.rsplit_once('.').map_or("", |(p, _)| p);
        let candidate = if parent.is_empty() {
            own.to_owned()
        } else {
            format!("{parent}.{own}")
        };
        if self.is_group_named(src, group) && self.is_group_named(src, &candidate) {
            format!("{candidate}.")
        } else {
            prefix.to_owned()
        }
    }

    /// Whether `obj` has a group named `name`.
    fn is_group_named(&self, obj: ObjectId, name: &str) -> bool {
        let Ok(p) = self.property_index(obj, name) else {
            return false;
        };
        self.meta_of(obj)
            .and_then(|m| {
                m.property(p)
                    .map(|i| i.flags.contains(PropertyFlags::GROUP))
            })
            .unwrap_or(false)
    }

    /// Writes each member the source gave a value, and takes back each
    /// other one that was given one here. `tracked`: in the effect that
    /// follows the source, which runs again when what it gave changes.
    fn copy_members(
        &self,
        obj: ObjectId,
        src: ObjectId,
        pairs: &Pairs,
        tracked: bool,
    ) -> Result<(), EngineError> {
        if !self.is_alive(obj) {
            return Ok(());
        }
        let given_in_src = |f: &PropIndex| {
            given(if tracked {
                self.provenance_tracked(src, *f)
            } else {
                self.provenance(src, *f)
            })
        };
        for &(to, from, outer) in pairs {
            let from = from
                .filter(given_in_src)
                .or_else(|| from.and(outer).filter(given_in_src));
            match from {
                Some(f) => {
                    let v = self.read(src, f);
                    self.untrack(|| self.write_impl(obj, to, v, WriteKind::Imperative, true))?;
                }
                None => {
                    if given(self.provenance(obj, to)) {
                        self.untrack(|| self.reset_member(obj, to))?;
                    }
                }
            }
        }
        Ok(())
    }
}
