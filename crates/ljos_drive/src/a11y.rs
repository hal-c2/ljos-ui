//! The scene as an assistive technology reads it, and where an input aimed
//! at one of its nodes goes.

use ljos_a11y::{AccessibleNode, NodeKey, role_word};
use ljos_quick::QuickScene;
use ljos_render::kurbo::{Point, Rect};

use crate::{Refusal, aim};

/// The tree as of the last frame, under a window named `title`: the
/// driver sits below the controls, which know what a window's title is.
/// Only the window while the tree is off.
pub fn tree(scene: &QuickScene, title: String) -> AccessibleNode {
    AccessibleNode::window(
        scene.accessibility_tree(),
        title,
        scene.accessible_focus_node(),
    )
}

/// The first node, in tree order, with the role word `role` and the name
/// `name`.
pub fn node(scene: &QuickScene, role: &str, name: &str) -> Option<NodeKey> {
    let mut found = None;
    scene.accessibility_tree().walk(|key, _, d| {
        if found.is_none() && role_word(d.role) == role && d.name == name {
            found = Some(key);
        }
    });
    found
}

/// The middle of the node `key`, unless a person could not click there
/// ([`aim`]). A node the tree no longer has is not shown.
pub fn aim_at(scene: &QuickScene, key: NodeKey) -> Result<Point, Refusal> {
    let mut seen = None;
    scene.accessibility_tree().walk(|k, _, d| {
        if k == key {
            let b = d.bounds;
            seen = Some((Rect::new(b.x0, b.y0, b.x1, b.y1), d.disabled));
        }
    });
    let (bounds, disabled) = seen.ok_or(Refusal::Hidden)?;
    aim(bounds, disabled, scene.viewport())
}
