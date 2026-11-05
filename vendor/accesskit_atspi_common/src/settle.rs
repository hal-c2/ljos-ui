// Copyright 2026 The AccessKit Authors. All rights reserved.
// Licensed under the Apache License, Version 2.0 (found in
// the LICENSE-APACHE file) or the MIT license (found in
// the LICENSE-MIT file), at your option.

//! An assistive technology that changes a node's value reads it back
//! straight after, to see that the change took. The application applies
//! the action on its own thread and pushes a tree update later, so a call
//! that answered as soon as it sent the action would leave the old value in
//! the tree for that read. [`ValueChange`] lets the platform adapter answer
//! once the update has come instead.

use accesskit_consumer::{FullNodeId, NodeRef};
use std::{
    future::Future,
    pin::Pin,
    sync::{Mutex, Weak},
    task::{Context as TaskContext, Poll, Waker},
};

use crate::context::Context;

/// Wakes what waits for the next tree update of an adapter.
#[derive(Debug, Default)]
pub(crate) struct Updates(Mutex<Waiting>);

#[derive(Debug, Default)]
struct Waiting {
    wakers: Vec<Waker>,
    /// The adapter is gone: no update will come.
    closed: bool,
}

impl Updates {
    /// Returns whether an update can still come.
    fn register(&self, waker: &Waker) -> bool {
        let mut waiting = self.0.lock().unwrap();
        if !waiting.wakers.iter().any(|w| w.will_wake(waker)) {
            waiting.wakers.push(waker.clone());
        }
        !waiting.closed
    }

    /// Called after the tree has changed and its lock is released, so that
    /// a woken waiter reads the new tree.
    pub(crate) fn notify(&self) {
        let wakers = std::mem::take(&mut self.0.lock().unwrap().wakers);
        for waker in wakers {
            waker.wake();
        }
    }

    /// A waiter that gave up (its call timed out) leaves no waker behind to
    /// keep its task alive until the next update.
    fn forget(&self, waker: &Waker) {
        self.0.lock().unwrap().wakers.retain(|w| !w.will_wake(waker));
    }

    pub(crate) fn close(&self) {
        self.0.lock().unwrap().closed = true;
        self.notify();
    }
}

/// What a node holds that an action can change.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Held {
    value: Option<String>,
    numeric: Option<f64>,
}

impl Held {
    pub(crate) fn of(node: &NodeRef<'_>) -> Self {
        // `value()` leaves out the runs of a multiline input that has no
        // value of its own; its text is what an edit changes.
        let value = node.value().or_else(|| {
            node.supports_text_ranges()
                .then(|| node.document_range().text())
        });
        Self {
            value,
            numeric: node.numeric_value(),
        }
    }
}

/// Resolves once a tree update has given a node something other than what
/// it held when this was made, or the node or its adapter is gone.
///
/// It does not end by itself: an application may take an action and leave
/// the value as it was (a validator refused the text, a slider was already
/// at its end), so the adapter bounds the wait.
#[derive(Debug)]
pub struct ValueChange {
    context: Weak<Context>,
    id: FullNodeId,
    before: Held,
    /// The waker last registered, to take back when this is dropped.
    waker: Option<Waker>,
}

impl ValueChange {
    pub(crate) fn new(context: Weak<Context>, id: FullNodeId, before: Held) -> Self {
        Self {
            context,
            id,
            before,
            waker: None,
        }
    }

    /// Whether the node holds this text already; a change to it would
    /// bring no update to wait for.
    pub fn holds_text(&self, text: &str) -> bool {
        self.before.value.as_deref() == Some(text)
    }

    /// Whether the node holds this number already.
    pub fn holds_number(&self, value: f64) -> bool {
        self.before.numeric == Some(value)
    }
}

#[cfg(test)]
impl ValueChange {
    /// How many wakers wait on the node's adapter.
    pub(crate) fn waiting(&self) -> usize {
        self.context
            .upgrade()
            .map_or(0, |context| context.updates.0.lock().unwrap().wakers.len())
    }
}

impl Future for ValueChange {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<()> {
        let this = self.get_mut();
        let Some(context) = this.context.upgrade() else {
            return Poll::Ready(());
        };
        // Registered before the tree is read: an update that lands in
        // between is either seen here or wakes the waker.
        this.waker = Some(cx.waker().clone());
        if !context.updates.register(cx.waker()) {
            return Poll::Ready(());
        }
        let tree = context.read_tree();
        match tree.state().node_by_id(this.id) {
            Some(node) if Held::of(&node) == this.before => Poll::Pending,
            _ => Poll::Ready(()),
        }
    }
}

impl Drop for ValueChange {
    fn drop(&mut self) {
        if let (Some(waker), Some(context)) = (&self.waker, self.context.upgrade()) {
            context.updates.forget(waker);
        }
    }
}
