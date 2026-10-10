//! The scene's accessibility tree, written into egui's AccessKit tree.
//!
//! egui owns the window's tree and rebuilds it every pass, so while an
//! assistive technology listens the view repeats its nodes every pass. The
//! scene's tree is retained and incremental; only this copy is per pass,
//! and it does not happen at all while nothing listens.

use std::collections::HashMap;

use egui::accesskit::{self, Action};
use egui::{Id, Pos2, Rect, Ui, UiBuilder};
use ljos_a11y::{NodeKey, Tree};

/// AccessKit ids of the nodes of the last pass, for routing actions back.
pub(crate) type Emitted = HashMap<accesskit::NodeId, NodeKey>;

/// Whether egui is building an AccessKit tree this pass.
pub(crate) fn listening(ui: &Ui) -> bool {
    // A `Ui` always has a node of its own. Asking for an id egui has not
    // seen would make a stray node under the window, so ours is not asked.
    ui.ctx()
        .accesskit_node_builder(ui.unique_id(), |_| ())
        .is_some()
}

/// The egui id of a node: the same item in the same view keeps it for as
/// long as it lives, wherever it moves in the tree.
pub(crate) fn node_id(view: Id, key: NodeKey) -> Id {
    Id::new((view, key.0))
}

/// Writes every node of `tree` under a node for the view. `rect` is where
/// the scene is painted.
pub(crate) fn emit(ui: &mut Ui, view: Id, rect: Rect, tree: &Tree, emitted: &mut Emitted) {
    emitted.clear();
    let ctx = ui.ctx().clone();
    // egui learns a node's parent from the `Ui` that carries its id, so
    // every node gets one. They paint nothing and take no space.
    let mut host = ui.new_child(UiBuilder::new().id(view).max_rect(rect));
    // Parents come before children and siblings in order, which is the
    // order egui lists children in.
    tree.walk(|key, parent, data| {
        let id = node_id(view, key);
        let parent_id = if parent == NodeKey::ROOT {
            view
        } else {
            node_id(view, parent)
        };
        let b = data.bounds;
        let min = Pos2::new(rect.min.x + b.x0 as f32, rect.min.y + b.y0 as f32);
        let max = Pos2::new(rect.min.x + b.x1 as f32, rect.min.y + b.y1 as f32);
        let _node_ui = host.new_child(
            UiBuilder::new()
                .id(id)
                .max_rect(Rect::from_min_max(min, max))
                .accessibility_parent(parent_id),
        );
        // The closure runs under the context lock: it only copies.
        ctx.accesskit_node_builder(id, |node| {
            data.apply(node);
            // The scene has no key focus yet, so a focus request would do
            // nothing; do not offer it.
            node.remove_action(Action::Focus);
            // Logical points; egui's root node scales to pixels.
            node.set_bounds(accesskit::Rect {
                x0: f64::from(min.x),
                y0: f64::from(min.y),
                x1: f64::from(max.x),
                y1: f64::from(max.y),
            });
        });
        emitted.insert(id.accesskit_id(), key);
    });
}
