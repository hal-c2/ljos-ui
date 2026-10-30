//! Pictures of items for other crates: an item and its children drawn off
//! the screen, as a 3D texture shows a `Texture.sourceItem`.
//!
//! A reader asks for an item's picture inside a tracked read (a painted
//! item's [`ReadPaint`](crate::ReadPaint)) and gets the last one drawn, or
//! none before the first. The scene that shows the reader draws the item
//! at its frames: the item joins that scene, outside the window if it is
//! not in it already, so it is laid out and painted as any other, and is
//! drawn again only when what it shows changed. The new picture repaints
//! whoever read the old one. Nothing is drawn while nothing changes.

use std::cell::{Cell, RefCell};

use ljos_engine::{Engine, ObjectId, Value};
use ljos_reactive::NodeId;
use ljos_render::{ImageHandle, kurbo};
use rustc_hash::FxHashMap;
use smallvec::SmallVec;

/// Which part of an item its picture shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PictureArea {
    /// The item's own bounds, from its top-left corner to its size.
    Bounds,
    /// The rectangle its shown children cover, in its coordinates: for an
    /// item that holds others without being drawn itself.
    Children,
}

/// An item's picture.
#[derive(Clone, Debug)]
pub struct ItemPicture {
    /// The pixels, as many a logical pixel as the scene's scale factor.
    pub image: ImageHandle,
    /// What of the item it shows, in the item's logical coordinates.
    pub area: kurbo::Rect,
}

pub(crate) struct Entry {
    pub(crate) area: PictureArea,
    /// Read by the readers, notified when a new picture is drawn.
    pub(crate) node: NodeId,
    pub(crate) readers: SmallVec<[ObjectId; 1]>,
    pub(crate) shown: Option<ItemPicture>,
    /// What the shown picture was drawn from (`scene::fingerprint`).
    pub(crate) print: Option<u64>,
}

#[derive(Default)]
pub(crate) struct Pictures {
    pub(crate) entries: RefCell<FxHashMap<ObjectId, Entry>>,
    /// Bumped when an item is wanted that was not, or by another reader:
    /// a scene that has seen fewer looks for new items at its next frame.
    pub(crate) wants: Cell<u64>,
}

/// The last picture of `item` drawn for `reader`, an item in a scene.
/// Tracked: the read repaints when a new picture is drawn. The first ask
/// draws the item at the scene's next frame; until then there is none.
/// The item stays wanted while it and one of its readers are alive.
pub fn item_picture(
    engine: &Engine,
    reader: ObjectId,
    item: ObjectId,
    area: PictureArea,
) -> Option<ItemPicture> {
    let reg = engine.extension::<Pictures>();
    let node = {
        let mut entries = reg.entries.borrow_mut();
        let known = entries
            .get(&item)
            .is_some_and(|e| engine.graph().is_alive(e.node));
        if !known {
            // In the item's scope, so that it goes with the item.
            let scope = engine.object_scope(item)?;
            let node = engine.graph().property(scope, Value::Int(0)).ok()?;
            entries.insert(
                item,
                Entry {
                    area,
                    node,
                    readers: SmallVec::new(),
                    shown: None,
                    print: None,
                },
            );
        }
        let entry = entries.get_mut(&item)?;
        let mut wanted = !known;
        if entry.area != area {
            entry.area = area;
            entry.print = None;
            wanted = true;
        }
        if !entry.readers.contains(&reader) {
            entry.readers.push(reader);
            wanted = true;
        }
        if wanted {
            reg.wants.set(reg.wants.get() + 1);
        }
        (entry.node, wanted)
    };
    if node.1 {
        crate::scene::pictures_wanted(engine);
    }
    let _ = engine.graph().get(node.0);
    reg.entries.borrow().get(&item)?.shown.clone()
}

/// Whether an item was wanted since a scene saw `seen` wants.
pub(crate) fn wanted_since(engine: &Engine, seen: u64) -> bool {
    engine.extension::<Pictures>().wants.get() != seen
}
