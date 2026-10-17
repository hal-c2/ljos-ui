//! The end of a creation, in steps (ARCHITECTURE.md §6.3).
//!
//! A [`super::CreationJob`] applies its objects one per step. What comes
//! after is most of the cost of a component with many bindings: running
//! them, the native completion hooks and `Component.onCompleted`. Here
//! that is cut the same way, so a frame's slice can end between any two
//! of them. The order is the one of [`Engine::finalize`] and of
//! [`Engine::run_held`], and a job that is never asked to stop does
//! exactly what a creation made at once does.
//!
//! What a stop changes: the batch the bindings run in ends with the
//! slice, so change handlers of what has run so far are called before the
//! remaining bindings run, not after all of them.

use std::collections::HashSet;

use super::{Creation, Initial};
use crate::engine::Engine;
use crate::error::EngineError;
use crate::ids::ObjectId;
use crate::value::Value;

pub(super) struct Finish {
    cr: Creation,
    root: ObjectId,
    stage: Stage,
    /// Something ran since the slice was last looked at. The look is
    /// taken before the next thing runs, not after this one: a creation
    /// with nothing left ends in the slice it is in and does not wait a
    /// frame to find that out.
    due: bool,
}

enum Stage {
    /// Initial properties, then the bindings that are not on a bindable
    /// property of Qt's, from the last set up to the first.
    Evaluate {
        props: Option<Vec<(String, Value)>>,
        how: Initial,
        /// Indices into `cr.held` and how many of them have run.
        todo: Option<(Vec<usize>, usize)>,
        result: Result<(), EngineError>,
    },
    /// The bindings on bindable properties, each in a batch of its own.
    Release {
        order: Vec<usize>,
        next: usize,
    },
    /// The required and assignable checks, and what a dev reload keeps.
    Check(Result<(), EngineError>),
    /// Native completion hooks, the last made first; `left` are to go.
    Hooks {
        kept: HashSet<ObjectId>,
        left: usize,
    },
    /// `Component.onCompleted`, or `Component.onReloaded` for the kept.
    Handlers {
        kept: HashSet<ObjectId>,
        stack: Vec<ObjectId>,
        reloaded: bool,
    },
    Gone,
}

impl Finish {
    pub(super) fn new(
        cr: Creation,
        root: ObjectId,
        props: Vec<(String, Value)>,
        how: Initial,
    ) -> Self {
        Self {
            cr,
            root,
            stage: Stage::Evaluate {
                props: Some(props),
                how,
                todo: None,
                result: Ok(()),
            },
            due: false,
        }
    }

    pub(super) fn root(&self) -> ObjectId {
        self.root
    }

    /// Goes on until the creation is through (`Some`) or `check` says the
    /// slice is spent (`None`). `check` is asked between two bindings,
    /// hooks or objects' handlers that run, and not otherwise: what is
    /// skipped costs nothing worth a look at the clock.
    pub(super) fn step(
        &mut self,
        e: &Engine,
        check: &dyn Fn() -> bool,
    ) -> Option<Result<ObjectId, EngineError>> {
        let g = &e.inner.graph;
        // Whether to stop before the next thing runs.
        let spent = |due: &mut bool| {
            let stop = *due && check();
            if stop {
                *due = false;
            }
            stop
        };
        loop {
            match std::mem::replace(&mut self.stage, Stage::Gone) {
                Stage::Evaluate {
                    mut props,
                    how,
                    mut todo,
                    mut result,
                } => {
                    let cr = &self.cr;
                    let root = self.root;
                    let due = &mut self.due;
                    // One batch for the initial properties and the
                    // bindings, as when the object is made at once.
                    let stopped = e.batch(|| {
                        if let Some(p) = props.take() {
                            let url = cr.top.as_ref().map(|t| t.file.url.clone());
                            result = e.set_initial(&url.unwrap_or_default(), root, &p, how);
                        }
                        let (order, next) =
                            todo.get_or_insert_with(|| (e.held_order(cr, false), 0));
                        while *next < order.len() {
                            let n = cr.held[order[*next]].node;
                            if !g.is_bound(n) {
                                *next += 1;
                                continue;
                            }
                            if spent(due) {
                                return true;
                            }
                            *next += 1;
                            if g.set_eager(n, true).is_ok() {
                                g.untrack(|| g.try_get(n));
                                *due = true;
                            }
                        }
                        false
                    });
                    if stopped {
                        self.stage = Stage::Evaluate {
                            props,
                            how,
                            todo,
                            result,
                        };
                        return None;
                    }
                    self.stage = match result {
                        Ok(()) => Stage::Release {
                            order: e.held_order(&self.cr, true),
                            next: 0,
                        },
                        Err(err) => Stage::Check(Err(err)),
                    };
                }
                Stage::Release { order, mut next } => {
                    while next < order.len() {
                        let n = self.cr.held[order[next]].node;
                        if !g.is_held(n) {
                            next += 1;
                            continue;
                        }
                        if spent(&mut self.due) {
                            self.stage = Stage::Release { order, next };
                            return None;
                        }
                        next += 1;
                        g.batch(|| {
                            if g.release(n).is_ok() && g.set_eager(n, true).is_ok() {
                                g.untrack(|| g.try_get(n));
                            }
                        });
                        self.due = true;
                    }
                    self.stage = Stage::Check(Ok(()));
                }
                Stage::Check(applied) => {
                    if let Err(err) = applied
                        .and_then(|()| e.check_required(&self.cr))
                        .and_then(|()| e.check_assignable())
                    {
                        e.reload_discard(self.root);
                        return Some(Err(err));
                    }
                    self.stage = Stage::Hooks {
                        kept: e.reload_adopt(&self.cr),
                        left: self.cr.all.len(),
                    };
                }
                Stage::Hooks { kept, mut left } => {
                    while left > 0 {
                        let o = self.cr.all[left - 1];
                        if !e.is_alive(o) {
                            left -= 1;
                            continue;
                        }
                        if spent(&mut self.due) {
                            self.stage = Stage::Hooks { kept, left };
                            return None;
                        }
                        left -= 1;
                        if let Err(err) = e.complete(o) {
                            e.warn(err.to_string());
                        }
                        self.due = true;
                    }
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
                        let handlers = match (reloaded, kept.contains(&o)) {
                            (false, false) => node.handlers.as_slice(),
                            (true, true) => node.reloaded.as_slice(),
                            _ => &[],
                        };
                        if !handlers.is_empty() && e.is_alive(o) {
                            if spent(&mut self.due) {
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
