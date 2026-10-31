//! `QtQuick3D.Physics`: a world that moves the nodes of a scene.
//!
//! The simulating is [`ljos_physics`]'s, and nothing of it shows outside this
//! crate: an application that does not ask for the module does not link the
//! engine. Everything here is reactive. A body says what it is by one effect
//! that reads its shapes, material and the rest ([`bodies::Spec`]), and the
//! world wakes when one changes; while nothing can move, the world asks for
//! no frames at all.

mod bodies;
mod shapes;
mod world;

pub use bodies::{
    CharacterController, DynamicRigidBody, PhysicsBody, PhysicsMaterial, PhysicsNode,
    StaticRigidBody, TriggerBody,
};
pub use shapes::{
    BoxShape, CapsuleShape, CollisionShape, ConvexMeshShape, HeightFieldShape, MeshShape,
    PlaneShape, SphereShape, TriangleMeshShape,
};
pub use world::PhysicsWorld;

/// Registers the `QtQuick3D.Physics` module with an engine.
pub fn register(engine: &ljos::Engine) {
    engine.register_module(ljos::module!("QtQuick3D.Physics" 6.0 => [
        PhysicsWorld, PhysicsMaterial, PhysicsNode, PhysicsBody, StaticRigidBody,
        DynamicRigidBody, TriggerBody, CharacterController, CollisionShape, BoxShape,
        SphereShape, CapsuleShape, PlaneShape, MeshShape, ConvexMeshShape,
        TriangleMeshShape, HeightFieldShape,
    ]));
}
