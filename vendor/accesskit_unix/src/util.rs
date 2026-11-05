// Copyright 2022 The AccessKit Authors. All rights reserved.
// Licensed under the Apache License, Version 2.0 (found in
// the LICENSE-APACHE file) or the MIT license (found in
// the LICENSE-MIT file), at your option.

use accesskit_atspi_common::{Error as InternalError, PlatformNode, ValueChange};
use std::time::Duration;
use zbus::fdo::Error as FdoError;

use crate::atspi::ObjectId;

#[cfg(not(feature = "tokio"))]
pub(crate) fn block_on<F: std::future::Future>(future: F) -> F::Output {
    futures_lite::future::block_on(future)
}

#[cfg(feature = "tokio")]
pub(crate) fn block_on<F: std::future::Future>(future: F) -> F::Output {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("launch of single-threaded tokio runtime");
    runtime.block_on(future)
}

/// How long a call that changes a node's value waits for the application
/// to show the change in a tree update. An application that took the
/// action but kept the value (a validator refused the text, a slider was
/// at its end) sends no such update; the call answers after this.
pub(crate) const ACTION_SETTLE_TIMEOUT: Duration = Duration::from_millis(500);

/// Answers once the application has shown the change, so that an
/// assistive technology reading the value straight after the call reads
/// the new one. zbus runs each method call as its own task, so a method's
/// wait holds up no other call. A property setter's does: zbus holds the
/// object server's root read lock across the setter, so registering or
/// removing a node in that time (which takes the write lock) waits for it,
/// and every call dispatched after that waits behind the writer. The wait
/// is bounded by [`ACTION_SETTLE_TIMEOUT`] and ends early once the update
/// that shows the change comes. The timer is async-io's under either
/// runtime feature: zbus runs the calls on its own async-io executor.
pub(crate) async fn settle(change: ValueChange) {
    futures_lite::future::or(change, async {
        async_io::Timer::after(ACTION_SETTLE_TIMEOUT).await;
    })
    .await
}

pub(crate) fn map_error(source: ObjectId, error: InternalError) -> FdoError {
    match error {
        InternalError::Defunct | InternalError::UnsupportedInterface => {
            FdoError::UnknownObject(source.path().to_string())
        }
        InternalError::TooManyChildren => FdoError::Failed("Too many children.".into()),
        InternalError::IndexOutOfRange => FdoError::Failed("Index is too big.".into()),
        InternalError::TooManyCharacters => FdoError::Failed("Too many characters.".into()),
        InternalError::UnsupportedTextGranularity => {
            FdoError::Failed("Unsupported text granularity.".into())
        }
    }
}

pub(crate) fn map_error_from_node(source: &PlatformNode, error: InternalError) -> FdoError {
    map_error(ObjectId::from(source), error)
}

#[cfg(test)]
mod tests {
    use super::{ACTION_SETTLE_TIMEOUT, settle};
    use accesskit::{ActionHandler, ActionRequest, Node, NodeId, Role, TreeId, TreeInfo, TreeUpdate};
    use accesskit_atspi_common::{
        Adapter, AdapterCallback, AppContext, Event, FullNodeId, InterfaceSet, WindowBounds,
    };
    use std::time::{Duration, Instant};

    struct NoOpCallback;

    impl AdapterCallback for NoOpCallback {
        fn register_interfaces(&self, _: &Adapter, _: FullNodeId, _: InterfaceSet) {}
        fn unregister_interfaces(&self, _: &Adapter, _: FullNodeId, _: InterfaceSet) {}
        fn emit_event(&self, _: &Adapter, _: Event) {}
    }

    struct NoOpHandler;

    impl ActionHandler for NoOpHandler {
        fn do_action(&mut self, _: ActionRequest) {}
    }

    const WINDOW: NodeId = NodeId(0);
    const SLIDER: NodeId = NodeId(1);

    fn slider(value: f64) -> Node {
        let mut node = Node::new(Role::Slider);
        node.set_numeric_value(value);
        node
    }

    fn adapter() -> Adapter {
        let mut window = Node::new(Role::Window);
        window.set_children(vec![SLIDER]);
        Adapter::new(
            &AppContext::new(None),
            NoOpCallback,
            TreeUpdate {
                nodes: vec![(WINDOW, window), (SLIDER, slider(20.0))],
                tree: Some(TreeInfo::new(WINDOW)),
                tree_id: TreeId::ROOT,
                focus: WINDOW,
            },
            false,
            WindowBounds::default(),
            NoOpHandler,
        )
    }

    fn the_slider(adapter: &Adapter) -> accesskit_atspi_common::PlatformNode {
        let window = adapter.platform_node(adapter.root_id());
        let id = window.child_at_index(0).unwrap().unwrap();
        adapter.platform_node(id)
    }

    #[test]
    fn a_change_that_never_shows_answers_after_the_timeout() {
        let adapter = adapter();
        let change = the_slider(&adapter).value_change().unwrap();
        let start = Instant::now();
        futures_lite::future::block_on(settle(change));
        assert!(start.elapsed() >= ACTION_SETTLE_TIMEOUT);
    }

    #[test]
    fn a_change_answers_when_the_update_that_shows_it_comes() {
        let mut adapter = adapter();
        let change = the_slider(&adapter).value_change().unwrap();
        let (sent, ready) = std::sync::mpsc::channel();
        let app = std::thread::spawn(move || {
            ready.recv().unwrap();
            adapter.update(TreeUpdate {
                nodes: vec![(SLIDER, slider(70.0))],
                tree: None,
                tree_id: TreeId::ROOT,
                focus: WINDOW,
            });
            adapter
        });
        let start = Instant::now();
        futures_lite::future::block_on(async {
            sent.send(()).unwrap();
            settle(change).await
        });
        // Well below the timeout: the update, not the timer, ended it.
        assert!(start.elapsed() < ACTION_SETTLE_TIMEOUT - Duration::from_millis(250));
        drop(app.join().unwrap());
    }
}
