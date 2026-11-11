//! Making an object from the moment all of it exists, in steps
//! (ARCHITECTURE.md §6.3).
//!
//! Every creation goes through [`Making`]. One made at once
//! ([`Engine::make_now`]) runs it with a check that never says stop, and a
//! [`super::CreationJob`] runs it a frame's slice at a time. There is one
//! list of what happens and in which order, so a job that is never asked
//! to stop does what a creation made at once does by construction:
//!
//! 1. **Settle**, in one batch. The aliases are wired, each object's
//!    properties, bindings and handlers are applied, and then the initial
//!    properties are set, with the bindings installed meanwhile held back
//!    ([`Engine::hold_eager`]). Then the held bindings that are not on a
//!    bindable property of Qt's run, from the last set up to the first.
//! 2. **Release**: the bindings on bindable properties, each in a batch of
//!    its own.
//! 3. **Check**: the required and assignable checks, and what a dev reload
//!    keeps.
//! 4. **Hooks**: the native completion hooks, the last made first.
//! 5. **Handlers**: `Component.onCompleted` and what listens to the
//!    attached object, or `Component.onReloaded` for what a reload kept.
//!
//! What a stop changes: the batch the first stage runs in ends with the
//! slice, so change handlers of what has run so far are called before the
//! rest of it runs, not after all of it.

use rustc_hash::FxHashSet as HashSet;

use super::{Creation, Initial};
use crate::engine::Engine;
use crate::error::EngineError;
use crate::ids::ObjectId;
use crate::meta::MetaRef;
use crate::object::QmlType;
use crate::qtqml::Component;
use crate::value::Value;

pub(super) struct Making {
    cr: Creation,
    root: ObjectId,
    stage: Stage,
    /// Something ran since the slice was last looked at. The look is
    /// taken before the next thing runs, not after this one: a creation
    /// with nothing left ends in the slice it is in and does not wait a
    /// frame to find that out.
    due: bool,
    /// An initial property was not set, and the object is not to be had
    /// ([`super::initial::Verdict::Withhold`]).
    withheld: bool,
}

enum Stage {
    Settle {
        /// The aliases are wired.
        aliased: bool,
        /// The next of `cr.units` to apply.
        next: usize,
        props: Vec<(String, Value)>,
        how: Initial,
        /// Once the initial properties are set: indices into `cr.held`
        /// and how many of them have run.
        todo: Option<(Vec<usize>, usize)>,
        /// Boxed, as in `Check`: an error is rare and large, and the
        /// stage is moved at each step.
        failed: Option<Box<EngineError>>,
    },
    Release {
        order: Vec<usize>,
        next: usize,
    },
    /// What went wrong before it, if anything.
    Check(Option<Box<EngineError>>),
    /// `left` of `cr.all` are to go, from the last.
    Hooks {
        kept: HashSet<ObjectId>,
        left: usize,
    },
    Handlers {
        kept: HashSet<ObjectId>,
        stack: Vec<ObjectId>,
        /// The second walk, for what a reload kept.
        reloaded: bool,
    },
    Gone,
}

/// Whether to stop before the next thing runs.
fn spent(due: &mut bool, check: &dyn Fn() -> bool) -> bool {
    let stop = *due && check();
    if stop {
        *due = false;
    }
    stop
}

impl Making {
    /// `cr` is allocated and `root` is its root; `props` are set as `how`
    /// says once every object is applied.
    pub(super) fn new(
        cr: Creation,
        root: ObjectId,
        props: Vec<(String, Value)>,
        how: Initial,
    ) -> Self {
        Self {
            cr,
            root,
            stage: Stage::Settle {
                aliased: false,
                next: 0,
                props,
                how,
                todo: None,
                failed: None,
            },
            due: false,
            withheld: false,
        }
    }

    pub(super) fn root(&self) -> ObjectId {
        self.root
    }

    pub(super) fn withheld(&self) -> bool {
        self.withheld
    }

    /// Applied objects over all objects, while they are being applied.
    pub(super) fn progress(&self) -> Option<f64> {
        match &self.stage {
            Stage::Settle {
                next, todo: None, ..
            } if !self.cr.units.is_empty() => Some(*next as f64 / self.cr.units.len() as f64),
            _ => None,
        }
    }

    /// Goes on until the creation is through (`Some`) or `check` says the
    /// slice is spent (`None`). `check` is asked between two objects
    /// applied, and between two bindings, hooks or objects' handlers that
    /// run: what is skipped costs nothing worth a look at the clock.
    pub(super) fn step(
        &mut self,
        e: &Engine,
        check: &dyn Fn() -> bool,
    ) -> Option<Result<ObjectId, EngineError>> {
        loop {
            match std::mem::replace(&mut self.stage, Stage::Gone) {
                Stage::Settle {
                    mut aliased,
                    mut next,
                    props,
                    how,
                    mut todo,
                    mut failed,
                } => {
                    let cr = &mut self.cr;
                    let root = self.root;
                    let due = &mut self.due;
                    let withheld = &mut self.withheld;
                    // Qt 6.11: an initial property's handler runs as the
                    // property is written, before the object's bindings
                    // run; a binding the handler reads runs there, its own
                    // handler inside, and one a handler makes stale before
                    // it ran runs once ("eval3 dbl6 w3/6 evtri tri8" in
                    // features/components/creation-order.feature). Ljós
                    // settles the properties and bindings in this batch,
                    // and the handlers run after it in height order (#331).
                    let stopped = e.batch(|| {
                        if todo.is_none() {
                            let (stop, held) = e.hold_eager(|| {
                                if !aliased {
                                    aliased = true;
                                    e.apply_aliases(cr);
                                }
                                while next < cr.units.len() {
                                    e.apply_unit(cr, next);
                                    next += 1;
                                    if next < cr.units.len() && check() {
                                        return true;
                                    }
                                }
                                if !props.is_empty() {
                                    let url = cr.top.as_ref().map(|t| t.file.url.clone());
                                    match e.set_initial(
                                        &url.unwrap_or_default(),
                                        root,
                                        &props,
                                        &how,
                                    ) {
                                        Ok(s) => *withheld = s.withheld,
                                        Err(err) => failed = Some(Box::new(err)),
                                    }
                                }
                                false
                            });
                            cr.held.extend(held);
                            if stop {
                                return true;
                            }
                            todo = Some((e.held_order(cr, false), 0));
                        }
                        let Some((order, next)) = &mut todo else {
                            return false;
                        };
                        e.run_held_from(cr, order, next, due, check)
                    });
                    if stopped {
                        self.stage = Stage::Settle {
                            aliased,
                            next,
                            props,
                            how,
                            todo,
                            failed,
                        };
                        return None;
                    }
                    // Outside the batch: see [`Engine::release_held_from`].
                    self.stage = match failed {
                        None => Stage::Release {
                            order: e.held_order(&self.cr, true),
                            next: 0,
                        },
                        Some(err) => Stage::Check(Some(err)),
                    };
                }
                Stage::Release { order, mut next } => {
                    if e.release_held_from(&self.cr, &order, &mut next, &mut self.due, check) {
                        self.stage = Stage::Release { order, next };
                        return None;
                    }
                    self.stage = Stage::Check(None);
                }
                Stage::Check(failed) => {
                    if let Err(err) = failed
                        .map_or(Ok(()), |err| Err(*err))
                        .and_then(|()| e.check_required(&self.cr))
                        .and_then(|()| e.check_assignable())
                    {
                        e.reload_discard(self.root);
                        return Some(Err(err));
                    }
                    // Dev reload: the objects that go on for old ones
                    // take what those held before anything completes,
                    // and are not completed in the file's sense a second
                    // time.
                    self.stage = Stage::Hooks {
                        kept: e.reload_adopt(&self.cr),
                        left: self.cr.all.len(),
                    };
                }
                Stage::Hooks { kept, mut left } => {
                    // The last made first, so children are complete
                    // before the objects that lay them out.
                    while left > 0 {
                        let o = self.cr.all[left - 1];
                        if !e.is_alive(o) {
                            left -= 1;
                            continue;
                        }
                        if spent(&mut self.due, check) {
                            self.stage = Stage::Hooks { kept, left };
                            return None;
                        }
                        left -= 1;
                        if let Err(err) = e.complete(o) {
                            e.warn(err.to_string());
                        }
                        self.due = true;
                    }
                    e.note_scope_sizes(&self.cr);
                    self.stage = Stage::Handlers {
                        kept,
                        stack: vec![self.root],
                        reloaded: false,
                    };
                }
                Stage::Handlers {
                    kept,
                    mut stack,
                    reloaded,
                } => {
                    while let Some(o) = stack.pop() {
                        let Some(node) = self.cr.nodes.get(&o) else {
                            continue;
                        };
                        // An object a dev reload kept is told it was
                        // reloaded and is not completed a second time.
                        let (handlers, completed) = match (reloaded, kept.contains(&o)) {
                            (false, false) => (node.handlers.as_slice(), true),
                            (true, true) => (node.reloaded.as_slice(), false),
                            _ => (&[][..], false),
                        };
                        // Whatever else listens to the attached object: a
                        // `Connections { target: Component }`, a
                        // `connect()`.
                        let attached = || {
                            completed
                                .then(|| {
                                    e.existing_attached_object(
                                        o,
                                        &MetaRef::Static(<Component as QmlType>::static_meta()),
                                    )
                                })
                                .flatten()
                        };
                        if (!handlers.is_empty() && e.is_alive(o)) || attached().is_some() {
                            if spent(&mut self.due, check) {
                                // Its turn again in the next slice.
                                stack.push(o);
                                self.stage = Stage::Handlers {
                                    kept,
                                    stack,
                                    reloaded,
                                };
                                return None;
                            }
                            for (inst, expr) in handlers {
                                if e.is_alive(o) {
                                    e.call_expr(inst, *expr, &[]);
                                }
                            }
                            // Looked up again after the handlers: one that
                            // is the first to touch the attached object
                            // (`Component.completed.connect(f)`) makes it,
                            // and `f` is told.
                            if let Some(att) = attached() {
                                let _ = e.emit_by_name(att, "completed", &[]);
                            }
                            self.due = true;
                        }
                        stack.extend(node.children.iter().copied());
                    }
                    if reloaded || kept.is_empty() {
                        return Some(Ok(self.root));
                    }
                    self.stage = Stage::Handlers {
                        kept,
                        stack: vec![self.root],
                        reloaded: true,
                    };
                }
                Stage::Gone => return Some(Ok(self.root)),
            }
        }
    }
}

impl Engine {
    /// Eager mode: runs the bindings a creation installed, in the order
    /// Qt runs them (found by running qml6 6.11.2 with bindings that log),
    /// so that the same member of a binding loop is reported and the
    /// members are left with the same values. `order` is
    /// [`Engine::held_order`]'s, not bindable, and `next` how much of it
    /// has run. True when `check` stopped it.
    ///
    /// Qt sets an object up in the order of [`super::setup_order`] and
    /// then runs the bindings from the last one set up to the first; a
    /// binding that reads a property whose binding has not run yet runs
    /// that one first. That is this function. Bindings on Qt's bindable
    /// properties are not there yet while this happens (the graph holds
    /// them), so a read gives the old value; [`Engine::release_held_from`]
    /// brings them in afterwards.
    pub(super) fn run_held_from(
        &self,
        cr: &Creation,
        order: &[usize],
        next: &mut usize,
        due: &mut bool,
        check: &dyn Fn() -> bool,
    ) -> bool {
        let g = &self.inner.graph;
        while *next < order.len() {
            let n = cr.held[order[*next]].node;
            if !g.is_bound(n) {
                *next += 1;
                continue;
            }
            if spent(due, check) {
                return true;
            }
            *next += 1;
            if g.set_eager(n, true).is_ok() {
                g.untrack(|| g.try_get(n));
                *due = true;
            }
        }
        false
    }

    /// The second half of [`Engine::run_held_from`]: Qt installs the
    /// bindings on its bindable properties one by one in set-up order and
    /// runs each as it goes in; what read the property before runs again.
    /// Called outside the creation's batch so that those run before the
    /// next binding goes in, as in Qt. True when `check` stopped it.
    pub(super) fn release_held_from(
        &self,
        cr: &Creation,
        order: &[usize],
        next: &mut usize,
        due: &mut bool,
        check: &dyn Fn() -> bool,
    ) -> bool {
        let g = &self.inner.graph;
        while *next < order.len() {
            let n = cr.held[order[*next]].node;
            if !g.is_held(n) {
                *next += 1;
                continue;
            }
            if spent(due, check) {
                return true;
            }
            *next += 1;
            g.batch(|| {
                if g.release(n).is_ok() && g.set_eager(n, true).is_ok() {
                    g.untrack(|| g.try_get(n));
                }
            });
            *due = true;
        }
        false
    }

    /// [`Engine::run_held_from`] all of it, for what is not a [`Making`]:
    /// a dev reload growing an instance in place.
    pub(super) fn run_held(&self, cr: &Creation) {
        let order = self.held_order(cr, false);
        self.run_held_from(cr, &order, &mut 0, &mut false, &|| false);
    }

    /// [`Engine::release_held_from`] all of it; see [`Engine::run_held`].
    pub(super) fn release_held(&self, cr: &Creation) {
        let order = self.held_order(cr, true);
        self.release_held_from(cr, &order, &mut 0, &mut false, &|| false);
    }
}
