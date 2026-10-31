//! `LookAtNode`: a node that keeps facing another.

use ljos::prelude::*;

use crate::node::Node;
use crate::space::{self, Place};

/// Turns about X and Y to look along its -Z at `target`, and keeps doing
/// so as either node moves. With no target it keeps the rotation it has.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick3D.Helpers", name = "LookAtNode", extends = Node)]
pub struct LookAtNode {
    #[qml(base)]
    node: Node,
    #[qml(notify)]
    target: Option<ObjectId>,
}

#[qml_impl]
impl LookAtNode {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let me = cx.this;
        cx.engine.effect_in(me, move |e| {
            let Some(target) = e.get(me, LookAtNode::TARGET).ok().flatten() else {
                return;
            };
            let at = space::place(e, target).world.translation();
            // Where it stands comes from its parent and its position,
            // not its rotation: the turn it makes must not wake it again.
            let parent = space::parent_node(e, me).map_or(Place::SCENE, |p| space::place(e, p));
            let position = space::from_value(&Value::Vector3d(
                e.get(me, Node::POSITION).unwrap_or_default(),
            ));
            let from = parent.world.transform_point(position);
            e.untrack(|| space::aim(e, me, from, at));
        })?;
        Ok(())
    }
}
