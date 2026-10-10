//! `Loader`: registered with its properties; it does not create anything
//! yet.
//!
//! TODO(contract item 5): creation needs, from the engine,
//! `Engine::create_from_url(url, parent, initial_props)` backed by a host
//! `ComponentSource` (url → `Arc<ljos_ir::CompiledFile>`), a `Component`
//! value with `createObject(parent, props)` for `sourceComponent`, and an
//! incubation `Job` for `ljos_runtime::Scheduler` when `asynchronous` is
//! set. With those, `active`/`source`/`sourceComponent` changes create or
//! destroy `item`, parent it to the Loader, size it as Qt documents (the
//! Loader follows the item's implicit size unless the Loader is sized,
//! in which case the item is resized to it), and drive `status`,
//! `progress` and `loaded()`.

use ljos_engine::{ObjectId, QmlObject, Url, qml_impl};

use crate::enums::loader_status;
use crate::item::Item;

/// `Loader` (QtQuick).
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick", name = "Loader", extends = Item)]
pub struct Loader {
    #[qml(base)]
    pub item_base: Item,
    #[qml(notify)]
    pub source: Url,
    #[qml(notify)]
    pub source_component: Option<ObjectId>,
    #[qml(notify)]
    pub active: bool,
    #[qml(notify)]
    pub asynchronous: bool,
    #[qml(notify, readonly)]
    pub item: Option<ObjectId>,
    /// [`crate::enums::loader_status`].
    #[qml(notify, readonly)]
    pub status: i32,
    #[qml(notify, readonly)]
    pub progress: f64,
}

impl Default for Loader {
    fn default() -> Self {
        Self {
            item_base: Item::default(),
            source: Url::default(),
            source_component: None,
            active: true,
            asynchronous: false,
            item: None,
            status: loader_status::NULL,
            progress: 0.0,
        }
    }
}

#[qml_impl]
impl Loader {
    #[qml(signal)]
    fn loaded();
}
