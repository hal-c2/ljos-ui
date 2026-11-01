//! `QtQuick3D.Particles3D`: what QML says of particles, read into the plain
//! numbers of [`ljos_3d::particles`], which simulates them. A kind's particles
//! go to the scene as an instance table, so they are one draw of one mesh
//! however many there are; the sprites' mesh is one rectangle
//! (`qml/QtQuick3D/Particles3D/impl/SpriteDraw.qml`) with shaders that are
//! resources, as the effects' are.
//!
//! A system's clock is a hidden `FrameAnimation` that runs only while the
//! system runs: one that is stopped or paused schedules nothing.

mod affectors;
mod emitters;
mod kinds;
mod shapes;
mod system;

use ljos::prelude::*;

pub use affectors::{
    Affector3D, Attractor3D, Gravity3D, PointRotator3D, Repeller3D, ScaleAffector3D, Wander3D,
};
pub use emitters::{
    Direction3D, DynamicBurst3D, EmitBurst3D, ParticleEmitter3D, TargetDirection3D, TrailEmitter3D,
    VectorDirection3D,
};
pub use kinds::{
    LineParticle3D, ModelBlendParticle3D, ModelParticle3D, Particle3D, ParticleTable,
    SpriteParticle3D, SpriteSequence3D,
};
pub use shapes::{
    ParticleAbstractShape3D, ParticleCustomShape3D, ParticleModelShape3D, ParticleSceneShape3D,
    ParticleShape3D,
};
pub use system::{ParticleSystem3D, ParticleSystem3DLogging};

macro_rules! shaders {
    ($($name:literal),* $(,)?) => {
        [$((
            concat!("ljos-quick3d-particles:/", $name),
            include_str!(concat!("../../shaders/particles/", $name)),
        )),*]
    };
}

pub(crate) fn register(engine: &Engine) {
    let platform = Platform::of(engine);
    for (url, text) in shaders!["sprite.vert", "line.vert", "sprite.frag"] {
        platform.register_resource(url, text.as_bytes());
    }
    engine.register_module(ljos::module!("QtQuick3D.Particles3D" 6.0 => [
        ParticleSystem3D, ParticleSystem3DLogging,
        Particle3D, ModelParticle3D, SpriteParticle3D, LineParticle3D, ModelBlendParticle3D,
        SpriteSequence3D, ParticleTable,
        EmitBurst3D, DynamicBurst3D, ParticleEmitter3D, TrailEmitter3D,
        Direction3D, VectorDirection3D, TargetDirection3D,
        ParticleAbstractShape3D, ParticleShape3D, ParticleModelShape3D, ParticleCustomShape3D,
        ParticleSceneShape3D,
        Affector3D, Gravity3D, PointRotator3D, Wander3D, Attractor3D, Repeller3D, ScaleAffector3D,
    ]));
}

fn floats(v: Vector3d) -> [f32; 3] {
    [v.x, v.y, v.z]
}
