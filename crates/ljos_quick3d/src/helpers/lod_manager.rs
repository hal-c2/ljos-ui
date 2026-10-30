//! `LodManager`: picks which level-of-detail model of its children shows.

use ljos::prelude::*;

use crate::node::Node;

/// Kept, not acted on: the children all stay as they are declared. Picking
/// one by the camera's distance means writing the visibility of other
/// nodes from here, which the scene walk has no place for yet.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick3D.Helpers", name = "LodManager", extends = Node)]
pub struct LodManager {
    #[qml(base)]
    node: Node,
    #[qml(notify)]
    camera: Option<ObjectId>,
    /// Distances from the camera at which the next level takes over.
    #[qml(notify)]
    distances: Vec<f64>,
    #[qml(notify)]
    fade_distance: f64,
}

impl Default for LodManager {
    fn default() -> Self {
        LodManager {
            node: Node::default(),
            camera: None,
            distances: Vec::new(),
            fade_distance: 0.0,
        }
    }
}
