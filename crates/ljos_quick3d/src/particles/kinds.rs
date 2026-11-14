//! The kinds of particle: what each one looks like and how it is drawn.

use ljos::item::Vector4d;
use ljos::item::ljos_3d::instancing::Instance;
use ljos::item::ljos_3d::math::{Mat4, Quat, Vec3};
use ljos::item::ljos_3d::particles::{
    Align, Fade, LineLook, Lines, Look, Seen, Sequence, SequenceDirection, TexcoordMode,
};
use ljos::ljos_engine::WriteKind;
use ljos::prelude::*;

use crate::instancing::{self, Entry, Instancing};
use crate::node::{Model, Object3D, vec3};

fn particle_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![
        (
            "FadeType",
            vec![("FadeNone", 0), ("FadeOpacity", 1), ("FadeScale", 2)],
        ),
        (
            "AlignMode",
            vec![
                ("AlignNone", 0),
                ("AlignTowardsTarget", 1),
                ("AlignTowardsStartVelocity", 2),
            ],
        ),
        (
            "SortMode",
            vec![
                ("SortNone", 0),
                ("SortNewest", 1),
                ("SortOldest", 2),
                ("SortDistance", 3),
            ],
        ),
    ]
}

/// `ParticleTable`: the rows a kind of particle draws, one for each that is
/// there. What makes it is the kind; a model shows it as any other table.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQuick3D.Particles3D",
    name = "ParticleTable",
    extends = Instancing,
    uncreatable
)]
pub struct ParticleTable {
    #[qml(base)]
    pub instancing: Instancing,
    #[qml(notify, readonly)]
    pub instance_count: i32,
}

/// `Particle3D`: what all kinds of particle have.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick3D.Particles3D",
    name = "Particle3D",
    extends = Object3D,
    uncreatable,
    enums = particle_enums
)]
pub struct Particle3D {
    #[qml(base)]
    pub object3d: Object3D,
    #[qml(notify)]
    pub max_amount: i32,
    #[qml(notify)]
    pub color: Color,
    #[qml(notify)]
    pub color_variation: Vector4d,
    #[qml(notify)]
    pub unified_color_variation: bool,
    #[qml(notify, enumeration = "FadeType")]
    pub fade_in_effect: i32,
    #[qml(notify, enumeration = "FadeType")]
    pub fade_out_effect: i32,
    #[qml(notify)]
    pub fade_in_duration: i32,
    #[qml(notify)]
    pub fade_out_duration: i32,
    #[qml(notify, enumeration = "AlignMode")]
    pub align_mode: i32,
    #[qml(notify)]
    pub align_target_position: Vector3d,
    #[qml(notify)]
    pub has_transparency: bool,
    #[qml(notify, enumeration = "SortMode")]
    pub sort_mode: i32,
    /// Its rows, made when it is complete.
    #[qml(skip)]
    table: Option<ObjectId>,
    /// What was made in the system it came to, to show them.
    #[qml(skip)]
    drawn_in: Option<ObjectId>,
}

impl Default for Particle3D {
    fn default() -> Self {
        Particle3D {
            object3d: Object3D::default(),
            max_amount: 100,
            color: Color::from_rgba8(255, 255, 255, 255),
            color_variation: Vector4d::default(),
            unified_color_variation: false,
            fade_in_effect: 1,
            fade_out_effect: 1,
            fade_in_duration: 250,
            fade_out_duration: 250,
            align_mode: 0,
            align_target_position: Vector3d::default(),
            has_transparency: true,
            sort_mode: 0,
            table: None,
            drawn_in: None,
        }
    }
}

#[qml_impl]
impl Particle3D {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        let table = e.create_native(ParticleTable::default(), Some(me))?;
        cx.with_mut(|p: &mut Particle3D| p.table = Some(table))?;
        // Those that say so sort by how far they are, each frame, and which
        // of a kind's pixels are see-through is not known beforehand.
        e.effect_in(me, move |e| {
            let distance = e.get(me, Particle3D::SORT_MODE).unwrap_or(0) == 3;
            let see_through = e.get(me, Particle3D::HAS_TRANSPARENCY).unwrap_or(true);
            let write = |p: PropIndex, v: bool| {
                let _ = e.write(table, p, Value::Bool(v), WriteKind::Host);
            };
            write(Instancing::DEPTH_SORTING_ENABLED.index, distance);
            write(Instancing::HAS_TRANSPARENCY.index, see_through);
        })?;
        Ok(())
    }
}

/// `ModelParticle3D`: a model for each particle: the one `delegate` makes, and
/// any other whose `instancing` is `instanceTable`.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQuick3D.Particles3D",
    name = "ModelParticle3D",
    extends = Particle3D,
    default = "delegate"
)]
pub struct ModelParticle3D {
    #[qml(base)]
    pub particle: Particle3D,
    #[qml(notify, object = "Component")]
    pub delegate: Option<ObjectId>,
    #[qml(notify, readonly, object = "ParticleTable")]
    pub instance_table: Option<ObjectId>,
}

#[qml_impl]
impl ModelParticle3D {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        let table = cx.with(|p: &Particle3D| p.table)?;
        e.set_internal(me, ModelParticle3D::INSTANCE_TABLE, table)?;
        Ok(())
    }
}

fn sprite_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![(
        "BlendMode",
        vec![("SourceOver", 0), ("Screen", 1), ("Multiply", 2)],
    )]
}

/// `SpriteParticle3D`: a square for each particle. `lights` and
/// `castsReflections` are kept and do nothing: a sprite is not lit.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick3D.Particles3D",
    name = "SpriteParticle3D",
    extends = Particle3D,
    enums = sprite_enums
)]
pub struct SpriteParticle3D {
    #[qml(base)]
    pub particle: Particle3D,
    #[qml(notify, enumeration = "BlendMode")]
    pub blend_mode: i32,
    #[qml(notify, object = "Texture")]
    pub sprite: Option<ObjectId>,
    #[qml(notify, object = "SpriteSequence3D")]
    pub sprite_sequence: Option<ObjectId>,
    #[qml(notify)]
    pub billboard: bool,
    #[qml(notify)]
    pub particle_scale: f64,
    #[qml(notify, object = "Texture")]
    pub color_table: Option<ObjectId>,
    #[qml(notify, list)]
    pub lights: QmlList,
    #[qml(notify)]
    pub offset_x: f64,
    #[qml(notify)]
    pub offset_y: f64,
    #[qml(notify)]
    pub casts_reflections: bool,
}

impl Default for SpriteParticle3D {
    fn default() -> Self {
        SpriteParticle3D {
            particle: Particle3D::default(),
            blend_mode: 0,
            sprite: None,
            sprite_sequence: None,
            billboard: false,
            particle_scale: 5.0,
            color_table: None,
            lights: QmlList::default(),
            offset_x: 0.0,
            offset_y: 0.0,
            casts_reflections: true,
        }
    }
}

fn line_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![(
        "TexcoordMode",
        vec![("Absolute", 0), ("Relative", 1), ("Fill", 2)],
    )]
}

/// `LineParticle3D`: a line behind each particle, of `segmentCount` pieces.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick3D.Particles3D",
    name = "LineParticle3D",
    extends = SpriteParticle3D,
    enums = line_enums
)]
pub struct LineParticle3D {
    #[qml(base)]
    pub sprite: SpriteParticle3D,
    #[qml(notify)]
    pub segment_count: i32,
    #[qml(notify)]
    pub alpha_fade: f64,
    #[qml(notify)]
    pub scale_multiplier: f64,
    #[qml(notify)]
    pub texcoord_multiplier: f64,
    #[qml(notify)]
    pub length: f64,
    #[qml(notify)]
    pub length_variation: f64,
    #[qml(notify)]
    pub length_delta_min: f64,
    #[qml(notify)]
    pub eol_fade_out_duration: i32,
    #[qml(notify, enumeration = "TexcoordMode")]
    pub texcoord_mode: i32,
}

impl Default for LineParticle3D {
    fn default() -> Self {
        LineParticle3D {
            sprite: SpriteParticle3D::default(),
            segment_count: 1,
            alpha_fade: 0.0,
            scale_multiplier: 1.0,
            texcoord_multiplier: 1.0,
            length: -1.0,
            length_variation: 0.0,
            length_delta_min: 10.0,
            eol_fade_out_duration: 0,
            texcoord_mode: 0,
        }
    }
}

fn blend_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![
        (
            "ModelBlendMode",
            vec![("Explode", 0), ("Construct", 1), ("Transfer", 2)],
        ),
        (
            "ModelBlendEmitMode",
            vec![("Sequential", 0), ("Random", 1), ("Activation", 2)],
        ),
    ]
}

/// `ModelBlendParticle3D`: registered so a scene with one loads. It takes part
/// as a kind, and draws nothing.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick3D.Particles3D",
    name = "ModelBlendParticle3D",
    extends = Particle3D,
    enums = blend_enums
)]
pub struct ModelBlendParticle3D {
    #[qml(base)]
    pub particle: Particle3D,
    #[qml(notify, object = "Component")]
    pub delegate: Option<ObjectId>,
    #[qml(notify, object = "Node")]
    pub end_node: Option<ObjectId>,
    #[qml(notify, enumeration = "ModelBlendMode")]
    pub model_blend_mode: i32,
    #[qml(notify)]
    pub end_time: i32,
    #[qml(notify, object = "Node")]
    pub activation_node: Option<ObjectId>,
    #[qml(notify, enumeration = "ModelBlendEmitMode")]
    pub emit_mode: i32,
}

impl Default for ModelBlendParticle3D {
    fn default() -> Self {
        ModelBlendParticle3D {
            particle: Particle3D {
                max_amount: 0,
                fade_in_effect: 0,
                fade_out_effect: 0,
                ..Particle3D::default()
            },
            delegate: None,
            end_node: None,
            model_blend_mode: 0,
            end_time: 0,
            activation_node: None,
            emit_mode: 0,
        }
    }
}

fn sequence_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![(
        "AnimationDirection",
        vec![
            ("Normal", 0),
            ("Reverse", 1),
            ("Alternate", 2),
            ("AlternateReverse", 3),
            ("SingleFrame", 4),
        ],
    )]
}

/// `SpriteSequence3D`: a sprite of pictures side by side, shown one by one.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick3D.Particles3D", name = "SpriteSequence3D", enums = sequence_enums)]
pub struct SpriteSequence3D {
    #[qml(notify)]
    pub frame_count: i32,
    #[qml(notify)]
    pub frame_index: i32,
    #[qml(notify)]
    pub interpolate: bool,
    #[qml(notify)]
    pub duration: i32,
    #[qml(notify)]
    pub duration_variation: i32,
    #[qml(notify)]
    pub random_start: bool,
    #[qml(notify, enumeration = "AnimationDirection")]
    pub animation_direction: i32,
}

impl Default for SpriteSequence3D {
    fn default() -> Self {
        SpriteSequence3D {
            frame_count: 1,
            frame_index: 0,
            interpolate: true,
            duration: -1,
            duration_variation: 0,
            random_start: false,
            animation_direction: 0,
        }
    }
}

fn fade(v: i32) -> Fade {
    match v {
        0 => Fade::None,
        2 => Fade::Scale,
        _ => Fade::Opacity,
    }
}

/// What the kind `kind` looks like now. Tracked.
pub(super) fn look(e: &Engine, kind: ObjectId) -> Look {
    use Particle3D as P;
    let [r, g, b, a] = e.get(kind, P::COLOR).unwrap_or_default().to_rgba_f();
    let variation = e.get(kind, P::COLOR_VARIATION).unwrap_or_default();
    Look {
        max_amount: e.get(kind, P::MAX_AMOUNT).unwrap_or(0).max(0) as usize,
        color: [r as f32, g as f32, b as f32, a as f32],
        color_variation: [variation.x, variation.y, variation.z, variation.w],
        unified_color_variation: e.get(kind, P::UNIFIED_COLOR_VARIATION).unwrap_or(false),
        fade_in: fade(e.get(kind, P::FADE_IN_EFFECT).unwrap_or(1)),
        fade_out: fade(e.get(kind, P::FADE_OUT_EFFECT).unwrap_or(1)),
        fade_in_duration: e.get(kind, P::FADE_IN_DURATION).unwrap_or(0) as f32,
        fade_out_duration: e.get(kind, P::FADE_OUT_DURATION).unwrap_or(0) as f32,
        align: match e.get(kind, P::ALIGN_MODE).unwrap_or(0) {
            1 => Align::Target(vec3(
                e.get(kind, P::ALIGN_TARGET_POSITION).unwrap_or_default(),
            )),
            2 => Align::StartVelocity,
            _ => Align::None,
        },
        billboard: e.is::<SpriteParticle3D>(kind)
            && e.get(kind, SpriteParticle3D::BILLBOARD).unwrap_or(false),
    }
}

/// Which way the kind `kind` puts its rows. Tracked.
pub(super) fn sort_mode(e: &Engine, kind: ObjectId) -> i32 {
    e.get(kind, Particle3D::SORT_MODE).unwrap_or(0)
}

/// The sequence of a sprite kind. Tracked.
pub(super) fn sequence(e: &Engine, kind: ObjectId) -> Option<Sequence> {
    use SpriteSequence3D as S;
    if !e.is::<SpriteParticle3D>(kind) {
        return None;
    }
    let s = e
        .get(kind, SpriteParticle3D::SPRITE_SEQUENCE)
        .ok()
        .flatten()?;
    Some(Sequence {
        frame_count: e.get(s, S::FRAME_COUNT).unwrap_or(1).max(1) as u32,
        frame_index: e.get(s, S::FRAME_INDEX).unwrap_or(0).max(0) as u32,
        duration: e.get(s, S::DURATION).unwrap_or(-1) as f32,
        duration_variation: e.get(s, S::DURATION_VARIATION).unwrap_or(0) as f32,
        random_start: e.get(s, S::RANDOM_START).unwrap_or(false),
        direction: match e.get(s, S::ANIMATION_DIRECTION).unwrap_or(0) {
            1 => SequenceDirection::Reverse,
            2 => SequenceDirection::Alternate,
            3 => SequenceDirection::AlternateReverse,
            4 => SequenceDirection::SingleFrame,
            _ => SequenceDirection::Normal,
        },
    })
}

/// What the kind `kind` says of its lines, when it is lines. Tracked.
pub(super) fn line_look(e: &Engine, kind: ObjectId) -> Option<(LineLook, f32)> {
    use LineParticle3D as L;
    if !e.is::<LineParticle3D>(kind) {
        return None;
    }
    let width = e.get(kind, SpriteParticle3D::PARTICLE_SCALE).unwrap_or(0.0) as f32;
    let mode = match e.get(kind, L::TEXCOORD_MODE).unwrap_or(0) {
        1 => TexcoordMode::Relative,
        2 => TexcoordMode::Fill,
        _ => TexcoordMode::Absolute,
    };
    let multiplier = e.get(kind, L::TEXCOORD_MULTIPLIER).unwrap_or(1.0) as f32;
    // How much of the picture one unit along the line is. A picture's shape
    // is taken as square: what its size is, is not known here.
    let stretch = multiplier
        * if mode == TexcoordMode::Fill {
            1.0
        } else if width != 0.0 {
            1.0 / width
        } else {
            0.0
        };
    Some((
        LineLook {
            segments: e.get(kind, L::SEGMENT_COUNT).unwrap_or(1).max(1) as usize,
            width,
            length: e.get(kind, L::LENGTH).unwrap_or(-1.0) as f32,
            length_variation: e.get(kind, L::LENGTH_VARIATION).unwrap_or(0.0) as f32,
            delta_min: e.get(kind, L::LENGTH_DELTA_MIN).unwrap_or(10.0) as f32,
            eol_fade_out: e.get(kind, L::EOL_FADE_OUT_DURATION).unwrap_or(0) as f32,
            mode,
            offset: [
                e.get(kind, SpriteParticle3D::OFFSET_X).unwrap_or(0.0) as f32,
                e.get(kind, SpriteParticle3D::OFFSET_Y).unwrap_or(0.0) as f32,
            ],
            scale_multiplier: (e.get(kind, L::SCALE_MULTIPLIER).unwrap_or(1.0) as f32)
                .clamp(0.0, 2.0),
            alpha_fade: (e.get(kind, L::ALPHA_FADE).unwrap_or(0.0) as f32).clamp(0.0, 1.0),
        },
        stretch,
    ))
}

/// Has the kind `kind` draw in `system`, the first time it comes to one.
pub(super) fn attach(e: &Engine, kind: ObjectId, system: ObjectId) {
    let Ok((table, drawn_in)) = e.with(kind, |p: &Particle3D| (p.table, p.drawn_in)) else {
        return;
    };
    let Some(table) = table else {
        return;
    };
    if drawn_in.is_some() {
        return;
    }
    let _ = e.with_mut(kind, |p: &mut Particle3D| p.drawn_in = Some(system));
    if e.is::<SpriteParticle3D>(kind) {
        draw_sprites(e, kind, system, table);
    } else if e.is::<ModelParticle3D>(kind) {
        let _ = e.effect_in(kind, move |e| {
            let delegate = e.get(kind, ModelParticle3D::DELEGATE).ok().flatten();
            e.untrack(|| {
                if let Some(delegate) = delegate {
                    made_models(e, delegate, system, table);
                }
            });
        });
    }
}

fn add_to(e: &Engine, system: ObjectId, node: ObjectId) {
    let mut data = match e.read(system, Object3D::DATA.index) {
        Value::List(l) => l.to_vec(),
        _ => Vec::new(),
    };
    data.push(Value::Object(node));
    let _ = e.write_internal(
        system,
        Object3D::DATA.index,
        Value::list(data),
        WriteKind::Initial,
    );
}

fn made_models(e: &Engine, delegate: ObjectId, system: ObjectId, table: ObjectId) {
    let made = e.component_create_owned(delegate, system, |e, root| add_to(e, system, root));
    match made {
        Ok(root) => show_table(e, root, table),
        Err(why) => e.log(
            LogLevel::Warning,
            "qt.quick3d.particles",
            format!("ModelParticle3D: {why}"),
        ),
    }
}

/// Every model in what a delegate made shows the table.
fn show_table(e: &Engine, node: ObjectId, table: ObjectId) {
    if e.is::<Model>(node) {
        let _ = e.write(
            node,
            Model::INSTANCING.index,
            Value::Object(table),
            WriteKind::Host,
        );
    }
    if let Value::List(l) = e.read(node, Object3D::DATA.index) {
        for child in l.iter().filter_map(Value::as_object) {
            show_table(e, child, table);
        }
    }
}

fn draw_sprites(e: &Engine, kind: ObjectId, system: ObjectId, table: ObjectId) {
    let file = e
        .qml_type_file("QtQuick3D.Particles3D.impl", "SpriteDraw")
        .unwrap_or_default();
    let draw = match e.create_from_url(&file, Some(system), Vec::new()) {
        Ok(draw) => draw,
        Err(why) => {
            e.log(
                LogLevel::Warning,
                "qt.quick3d.particles",
                format!("SpriteParticle3D: {why}"),
            );
            return;
        }
    };
    add_to(e, system, draw);
    let _ = e.write(
        draw,
        Model::INSTANCING.index,
        Value::Object(table),
        WriteKind::Host,
    );
    let line = e.is::<LineParticle3D>(kind);
    let _ = e.write_by_name(draw, "line", Value::Bool(line));
    let _ = e.effect_in(kind, move |e| {
        use SpriteParticle3D as S;
        let sprite = e.get(kind, S::SPRITE).ok().flatten();
        let colors = e.get(kind, S::COLOR_TABLE).ok().flatten();
        let blend = e.get(kind, S::BLEND_MODE).unwrap_or(0);
        let billboard = e.get(kind, S::BILLBOARD).unwrap_or(false);
        let sequence = e.get(kind, S::SPRITE_SEQUENCE).ok().flatten();
        let (frames, blended) = sequence.map_or((1, false), |s| {
            (
                e.get(s, SpriteSequence3D::FRAME_COUNT).unwrap_or(1).max(1),
                e.get(s, SpriteSequence3D::INTERPOLATE).unwrap_or(true),
            )
        });
        let object = |o: Option<ObjectId>| o.map_or(Value::Null, Value::Object);
        e.untrack(|| {
            for (name, value) in [
                ("sprite", object(sprite)),
                ("colorTable", object(colors)),
                ("pictured", Value::Bool(sprite.is_some())),
                ("tabled", Value::Bool(colors.is_some())),
                ("blendMode", Value::Int(blend)),
                ("billboard", Value::Bool(billboard)),
                ("frames", Value::Real(f64::from(frames))),
                ("interpolate", Value::Real(if blended { 1.0 } else { 0.0 })),
            ] {
                let _ = e.write_by_name(draw, name, value);
            }
        });
    });
}

/// The turn Qt gives a sprite by its three angles, in degrees.
fn about(angles: Vec3) -> Quat {
    let half = std::f32::consts::PI / 360.0;
    let (sx, cx) = (angles.x * half).sin_cos();
    let (sy, cy) = (angles.y * half).sin_cos();
    let (sz, cz) = (angles.z * half).sin_cos();
    let (w, x, y, z) = (cx * cy, sx * cy, cx * sy, sx * sy);
    Quat {
        w: w * cz - z * sz,
        x: x * cz + y * sz,
        y: y * cz - x * sz,
        z: w * sz + z * cz,
    }
}

/// A row of a kind of model.
fn model_entry(s: &Seen) -> Entry {
    let [r, g, b, _] = s.color;
    Entry {
        position: s.position,
        scale: Vec3::ONE * s.scale,
        rotation: s.turn,
        color: Color::from_rgba8(r, g, b, s.alpha),
        custom: [0.0; 4],
    }
}

/// A row of a kind of sprite: `size` is the kind's particle scale, `frame` how
/// far through its pictures it is. The colour is as QML gave it: what an
/// unshaded material writes is what is seen.
fn sprite_instance(s: &Seen, size: f32, offset: [f32; 2], frame: f32) -> Instance {
    let turn = about(if s.aligned {
        s.turn.to_euler_degrees()
    } else {
        s.rotation
    });
    let across = s.scale * size;
    let at = s.position + Vec3::new(offset[0], offset[1], 0.0) * across;
    let [r, g, b, _] = s.color;
    let byte = |v: u8| f32::from(v) / 255.0;
    Instance {
        transform: Mat4::from_trs(at, turn, Vec3::ONE),
        color: [byte(r), byte(g), byte(b), byte(s.alpha)],
        custom: [across, s.age, frame, 0.0],
    }
}

/// The rows of a kind for what is seen of it now.
pub(super) fn fill_sprites(
    e: &Engine,
    kind: ObjectId,
    seen: &[(Seen, f32)],
) -> (Vec<Entry>, Vec<Instance>) {
    use SpriteParticle3D as S;
    let size = e.get(kind, S::PARTICLE_SCALE).unwrap_or(0.0) as f32;
    let offset = [
        e.get(kind, S::OFFSET_X).unwrap_or(0.0) as f32,
        e.get(kind, S::OFFSET_Y).unwrap_or(0.0) as f32,
    ];
    let instances = seen
        .iter()
        .map(|(s, frame)| sprite_instance(s, size, offset, *frame))
        .collect();
    (Vec::new(), instances)
}

/// The rows of a kind of model for what is seen of it now.
pub(super) fn fill_models(seen: &[(Seen, f32)]) -> (Vec<Entry>, Vec<Instance>) {
    let entries: Vec<Entry> = seen.iter().map(|(s, _)| model_entry(s)).collect();
    let instances = entries.iter().map(Entry::instance).collect();
    (entries, instances)
}

/// The rows of a kind of line: one for each piece.
pub(super) fn fill_lines(
    lines: &Lines,
    now: f64,
    look: &LineLook,
    stretch: f32,
) -> (Vec<Entry>, Vec<Instance>) {
    let instances = lines
        .pieces(now, look)
        .into_iter()
        .map(|p| {
            let way = (p.b - p.a) * (1.0 / 100.0);
            let middle = (p.a + p.b) * 0.5;
            Instance {
                transform: Mat4([
                    [way.x, way.y, way.z, 0.0],
                    [p.across.x, p.across.y, p.across.z, 0.0],
                    [0.0, 0.0, 1.0, 0.0],
                    [middle.x, middle.y, middle.z, 1.0],
                ]),
                color: p.color,
                custom: [
                    p.width_a,
                    p.width_b,
                    p.length_a * stretch,
                    p.length_b * stretch,
                ],
            }
        })
        .collect();
    (Vec::new(), instances)
}

/// Puts rows in the table of the kind `kind`. A table with nothing in it that
/// had nothing before tells nobody, so a system with nothing to show has the
/// scene do no work.
pub(super) fn show(e: &Engine, kind: ObjectId, rows: (Vec<Entry>, Vec<Instance>)) {
    let Ok(Some(table)) = e.with(kind, |p: &Particle3D| p.table) else {
        return;
    };
    let (entries, instances) = rows;
    let count = instances.len() as i32;
    let before = e.get(table, ParticleTable::INSTANCE_COUNT).unwrap_or(0);
    if count == 0 && before == 0 {
        return;
    }
    instancing::put(e, table, entries, instances);
    let _ = e.set_internal(table, ParticleTable::INSTANCE_COUNT, count);
    let _ = e.notify(table, ParticleTable::INSTANCE_COUNT.index);
}
