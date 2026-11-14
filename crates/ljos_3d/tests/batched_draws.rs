//! A thousand models that differ only in where they are and their colour
//! are drawn as one: one draw call, the picture of drawing each on its own
//! to the rounding of their normals, and a click meets the model drawn
//! where it clicked. A camera that moves, even one that changes which batch
//! is the nearer, writes none of their rows; a model that moves writes its
//! own. A model no longer alike, or with a depth bias, is drawn on its own;
//! of two alike in one place the later declared shows, as drawn apart;
//! one that writes no depth, or a biased one, stays between the models
//! alike around it, and two batches meeting in one place, or two models
//! whose faces meet at one depth, keep the order drawn apart has there; and a batched model's colour draws no shadow map
//! again.
//!
//! One test, in a process of its own: the counts are the process's, and a
//! second test drawing beside it would add to them.

use ljos_3d::math::{Quat, Vec3};
use ljos_3d::offscreen::{Rgba, read_back};
use ljos_3d::pick::{Viewport, pick};
use ljos_3d::{
    Background, Camera, Color, CullMode, DefaultMaterial, DepthDraw, Environment, Gpu, Light,
    Material, MeshSource, Model, Node, NodeId, NodeKind, Primitive, Scene, Shadow, Target,
    Transform, stats,
};

const SIZE: [u32; 2] = [320, 200];
const COLUMNS: usize = 40;
const ROWS: usize = 25;

fn color(i: usize) -> Color {
    let (c, r) = ((i % COLUMNS) as f32, (i / COLUMNS) as f32);
    Color::rgb(
        c / COLUMNS as f32,
        r / ROWS as f32,
        1.0 - c / COLUMNS as f32,
    )
}

/// The cubes in a grid, each turned its own way, and two spheres in front
/// of them: another mesh, so another batch. Seen from `camera`.
fn scene(camera: Transform) -> (Scene, Vec<NodeId>) {
    let mut scene = Scene::new();
    scene.set_environment(Environment {
        background: Background::Color(Color::rgb(0.1, 0.1, 0.1)),
        ..Environment::default()
    });
    let camera = scene.add(
        None,
        Node::new(NodeKind::Camera(Camera::PERSPECTIVE)).with_transform(camera),
    );
    scene.set_camera(Some(camera));
    scene.add(
        None,
        Node::new(NodeKind::Light(Light::DIRECTIONAL)).with_transform(Transform {
            rotation: Quat::from_euler_degrees(Vec3::new(-30.0, -40.0, 0.0)),
            ..Transform::default()
        }),
    );
    let cubes = (0..COLUMNS * ROWS)
        .map(|i| {
            let (c, r) = ((i % COLUMNS) as f32, (i / COLUMNS) as f32);
            scene.add(
                None,
                Node::new(NodeKind::Model(Model {
                    mesh: MeshSource::Primitive(Primitive::Cube),
                    material: Material::Default(DefaultMaterial {
                        diffuse_color: color(i),
                        ..DefaultMaterial::default()
                    }),
                }))
                .with_transform(Transform {
                    position: Vec3::new(-390.0 + 20.0 * c, -240.0 + 20.0 * r, 0.0),
                    rotation: Quat::from_euler_degrees(Vec3::new(9.0 * r, 7.0 * c, 0.0)),
                    scale: Vec3::new(0.12, 0.12, 0.12),
                }),
            )
        })
        .collect();
    for x in [-200.0, 200.0] {
        scene.add(
            None,
            Node::new(NodeKind::Model(Model {
                mesh: MeshSource::Primitive(Primitive::Sphere),
                material: Material::Default(DefaultMaterial::default()),
            }))
            .with_transform(Transform {
                position: Vec3::new(x, 0.0, 60.0),
                ..Transform::default()
            }),
        );
    }
    (scene, cubes)
}

/// A camera at `x` across, 700 in front of the grid, looking at it.
fn front(x: f32) -> Transform {
    Transform {
        position: Vec3::new(x, 0.0, 700.0),
        ..Transform::default()
    }
}

/// Draws `scene` into `target` as a host does, ending the frame after its
/// submit.
fn frame(gpu: &Gpu, target: &mut Target, scene: &Scene) -> (stats::FrameCounters, Rgba) {
    let mut encoder = gpu
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    let drew = gpu
        .renderer()
        .render(gpu.device(), gpu.queue(), &mut encoder, scene, target);
    assert!(drew, "a scene built anew is drawn");
    gpu.queue().submit([encoder.finish()]);
    stats::frame_end(0.0);
    let counts = stats::snapshot().last;
    (
        counts,
        read_back(gpu.device(), gpu.queue(), target.texture()).unwrap(),
    )
}

#[test]
fn models_alike_but_for_place_and_colour_are_one_draw() {
    let gpu = match pollster::block_on(Gpu::new()) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("SKIPPED: {e}");
            return;
        }
    };
    let models = COLUMNS as u64 * ROWS as u64 + 2;
    let (still, cubes) = scene(front(0.0));
    let mut target = Target::new(gpu.device(), &gpu.renderer(), SIZE);
    let (counts, batched) = frame(&gpu, &mut target, &still);
    assert_eq!(
        counts.draw_calls, 2,
        "the cubes and the spheres: {counts:?}"
    );
    assert_eq!(counts.models_drawn, models, "{counts:?}");

    // A camera that moves leaves the rows as they were: from in front the
    // spheres are the nearer batch, from behind the cubes are.
    let behind = Transform {
        position: Vec3::new(0.0, 0.0, -700.0),
        rotation: Quat::from_euler_degrees(Vec3::new(0.0, 180.0, 0.0)),
        ..Transform::default()
    };
    for camera in [front(15.0), behind] {
        let (moved, _) = frame(&gpu, &mut target, &scene(camera).0);
        assert_eq!(
            (moved.draw_calls, moved.models_drawn),
            (2, models),
            "{moved:?}"
        );
        assert!(
            moved.buffer_write_bytes < 8 * 1024,
            "only the camera's uniforms are written for {camera:?}: {moved:?}"
        );
    }

    // Drawn each on its own, the picture is the same but where a normal
    // came out a rounding apart.
    gpu.renderer().set_batching(false);
    let mut alone = Target::new(gpu.device(), &gpu.renderer(), SIZE);
    let (counts, each) = frame(&gpu, &mut alone, &still);
    gpu.renderer().set_batching(true);
    assert_eq!(counts.draw_calls, models, "{counts:?}");
    assert_within_a_level(&batched, &each, "each on its own");

    // A click meets the cube drawn where it clicked.
    let viewport = Viewport::of_scene(&still, SIZE[0] as f32, SIZE[1] as f32).unwrap();
    let background = batched.pixel(1, 1);
    for i in [0, 41, 517, 999] {
        let centre = still.world(cubes[i]).unwrap().translation();
        let at = viewport
            .from_scene(centre)
            .expect("the cube is in front of the camera");
        let ray = viewport.ray(at.x, at.y).unwrap();
        let hit = pick(&still, &ray, |_| true);
        assert_eq!(
            hit.first().map(|h| h.node),
            Some(cubes[i]),
            "cube {i} at {at:?}"
        );
        assert_ne!(
            batched.pixel(at.x as u32, at.y as u32),
            background,
            "cube {i} is drawn where a click meets it, at {at:?}"
        );
    }

    // A cube that moves writes its own row, not the others'.
    frame(&gpu, &mut target, &still);
    let mut one_moved = scene(front(0.0)).0;
    one_moved.node_mut(cubes[517]).unwrap().transform.position.y += 7.0;
    let (moved, after) = frame(&gpu, &mut target, &one_moved);
    assert_eq!(moved.draw_calls, 2, "{moved:?}");
    assert!(
        moved.buffer_write_bytes < 8 * 1024,
        "only the moved cube's row and the uniforms: {moved:?}"
    );
    let mut fresh = Target::new(gpu.device(), &gpu.renderer(), SIZE);
    assert_eq!(
        after,
        frame(&gpu, &mut fresh, &one_moved).1,
        "cube 517 moved"
    );

    // A cube whose material is no longer the others' leaves the batch, and
    // is drawn where it is on its own.
    let mut one_apart = scene(front(0.0)).0;
    match &mut one_apart.node_mut(cubes[600]).unwrap().kind {
        NodeKind::Model(Model {
            material: Material::Default(m),
            ..
        }) => m.specular_roughness = 0.5,
        _ => unreachable!("the cubes are models"),
    }
    let (apart, batched) = frame(&gpu, &mut target, &one_apart);
    assert_eq!(
        apart.draw_calls, 3,
        "the cubes, cube 600 and the spheres: {apart:?}"
    );
    gpu.renderer().set_batching(false);
    let mut alone = Target::new(gpu.device(), &gpu.renderer(), SIZE);
    let each = frame(&gpu, &mut alone, &one_apart).1;
    gpu.renderer().set_batching(true);
    assert_within_a_level(&batched, &each, "cube 600 apart");

    // Of two models in one place, the one whose depth bias puts it farther
    // is drawn last, and so over the other, batched or not.
    let biased = two_in_one_place();
    let mut two = Target::new(gpu.device(), &gpu.renderer(), SIZE);
    let (counts, batched) = frame(&gpu, &mut two, &biased);
    gpu.renderer().set_batching(false);
    let mut two = Target::new(gpu.device(), &gpu.renderer(), SIZE);
    let each = frame(&gpu, &mut two, &biased).1;
    gpu.renderer().set_batching(true);
    let [r, _, b, _] = batched.pixel(SIZE[0] / 2, SIZE[1] / 2);
    assert!(
        r > b,
        "the red cube, biased farther, shows: {r} red, {b} blue"
    );
    assert_within_a_level(&batched, &each, "depth bias");
    assert_eq!(
        counts.draw_calls, 2,
        "a biased model is drawn on its own: {counts:?}"
    );

    // A model that writes no depth between two alike is drawn between
    // them: the farther, drawn after it, covers it.
    let between = small(
        &[
            (
                Primitive::Cube,
                Vec3::new(-150.0, 0.0, 100.0),
                RED,
                DepthDraw::OpaqueOnly,
            ),
            (
                Primitive::Sphere,
                Vec3::new(0.0, 0.0, 0.0),
                GREEN,
                DepthDraw::Never,
            ),
            (
                Primitive::Cube,
                Vec3::new(0.0, 0.0, -100.0),
                BLUE,
                DepthDraw::OpaqueOnly,
            ),
        ],
        None,
    );
    let mut three = Target::new(gpu.device(), &gpu.renderer(), SIZE);
    let (counts, batched) = frame(&gpu, &mut three, &between);
    gpu.renderer().set_batching(false);
    let mut three = Target::new(gpu.device(), &gpu.renderer(), SIZE);
    let each = frame(&gpu, &mut three, &between).1;
    gpu.renderer().set_batching(true);
    let [_, g, b, _] = batched.pixel(SIZE[0] / 2, SIZE[1] / 2);
    assert!(
        b > g,
        "the blue cube, drawn last, covers the sphere: {g} green, {b} blue"
    );
    assert_within_a_level(&batched, &each, "a model writing no depth between");
    assert_eq!(counts.draw_calls, 3, "{counts:?}");

    // Of two alike in one place with no bias, the later declared is drawn
    // last and shows, batched or not, whichever slots they hold: red is
    // declared after blue but takes the slot a hidden cube left before it.
    let tie = |models: &[(Primitive, Vec3, Color, DepthDraw)]| {
        let scene = small(models, None);
        let hidden = scene
            .walk()
            .into_iter()
            .find(|(_, node, _)| matches!(node.kind, NodeKind::Model(_)))
            .map(|(id, _, _)| id)
            .unwrap();
        (scene, hidden)
    };
    let filler = (
        Primitive::Cube,
        Vec3::new(120.0, 0.0, 0.0),
        GREEN,
        DepthDraw::OpaqueOnly,
    );
    let blue = (
        Primitive::Cube,
        Vec3::new(0.0, 0.0, 0.0),
        BLUE,
        DepthDraw::OpaqueOnly,
    );
    let red = (blue.0, blue.1, RED, blue.3);
    let mut held = Target::new(gpu.device(), &gpu.renderer(), SIZE);
    frame(&gpu, &mut held, &tie(&[filler, blue]).0);
    let (mut gone, hidden) = tie(&[filler, blue]);
    gone.node_mut(hidden).unwrap().visible = false;
    frame(&gpu, &mut held, &gone);
    let (mut later, hidden) = tie(&[filler, blue, red]);
    later.node_mut(hidden).unwrap().visible = false;
    let (counts, batched) = frame(&gpu, &mut held, &later);
    assert_eq!(
        counts.draw_calls, 1,
        "blue and red are one draw: {counts:?}"
    );
    gpu.renderer().set_batching(false);
    let mut alone = Target::new(gpu.device(), &gpu.renderer(), SIZE);
    let each = frame(&gpu, &mut alone, &later).1;
    gpu.renderer().set_batching(true);
    let [r, _, b, _] = batched.pixel(SIZE[0] / 2, SIZE[1] / 2);
    assert!(r > b, "red, declared later, shows: {r} red, {b} blue");
    assert_within_a_level(&batched, &each, "a tie in one place");

    // A biased model is drawn alone where its bias sorts it, and no batch
    // reaches across it: red is alike the green one nearer, blue in red's
    // place is biased nearer than red but farther than green, so red, drawn
    // after blue, shows, batched or not.
    let mut decal = small(
        &[
            (blue.0, Vec3::new(-120.0, 0.0, 60.0), GREEN, blue.3),
            red,
            blue,
        ],
        None,
    );
    let biased = models_of(&decal)[2];
    decal.node_mut(biased).unwrap().depth_bias = -5.0;
    let (counts, batched, each) = both(&gpu, &decal);
    let [r, _, b, _] = batched.pixel(SIZE[0] / 2, SIZE[1] / 2);
    assert!(
        r > b,
        "red, drawn after the biased blue, shows: {r} red, {b} blue"
    );
    assert_within_a_level(&batched, &each, "a biased model between two alike");
    assert_eq!(counts.draw_calls, 3, "{counts:?}");

    // Two batches meeting in one place: blue's batch is the nearer, red's
    // the farther, and red in blue's place is declared after it. Drawn
    // apart, red is drawn after blue there and shows; no batch reaches
    // across the other there. The far green, alike blue and meeting
    // neither, still joins blue's batch: red set apart ends no other.
    let mut meeting = small(
        &[
            (blue.0, Vec3::new(-120.0, 0.0, 60.0), GREEN, blue.3),
            blue,
            red,
            (blue.0, Vec3::new(120.0, 0.0, -60.0), GREEN, blue.3),
        ],
        None,
    );
    for &i in &[1, 3] {
        let id = models_of(&meeting)[i];
        match &mut meeting.node_mut(id).unwrap().kind {
            NodeKind::Model(Model {
                material: Material::Default(m),
                ..
            }) => m.specular_roughness = 0.5,
            _ => unreachable!("models"),
        }
    }
    let (counts, batched, each) = both(&gpu, &meeting);
    let [r, _, b, _] = batched.pixel(SIZE[0] / 2, SIZE[1] / 2);
    assert!(
        r > b,
        "red, declared after blue in its place, shows: {r} red, {b} blue"
    );
    assert_within_a_level(&batched, &each, "two batches in one place");
    assert_eq!(counts.draw_calls, 3, "{counts:?}");

    // Two that meet at one depth keep the order drawn apart has, though
    // their centres differ: green, nearer, is alike red; blue, of another
    // material, is declared before red, and their front faces meet. Drawn
    // apart, red is drawn after blue and shows where they meet.
    let mut offset = small(
        &[
            (blue.0, Vec3::new(-120.0, 0.0, 60.0), GREEN, blue.3),
            blue,
            (blue.0, Vec3::new(20.0, 0.0, 0.0), RED, blue.3),
        ],
        None,
    );
    let id = models_of(&offset)[1];
    match &mut offset.node_mut(id).unwrap().kind {
        NodeKind::Model(Model {
            material: Material::Default(m),
            ..
        }) => m.specular_roughness = 0.5,
        _ => unreachable!("models"),
    }
    let (counts, batched, each) = both(&gpu, &offset);
    let [r, _, b, _] = batched.pixel(SIZE[0] / 2, SIZE[1] / 2);
    assert!(
        r > b,
        "red, declared after blue where they meet, shows: {r} red, {b} blue"
    );
    assert_within_a_level(&batched, &each, "two that meet with centres apart");
    assert_eq!(counts.draw_calls, 3, "{counts:?}");

    // Of two alike whose faces meet at one depth, the nearer by its centre
    // declared later: drawn apart, the farther is drawn after it and shows;
    // one batch would lay their rows the other way round.
    let mut turned = small(
        &[
            (blue.0, Vec3::new(0.0, 0.0, -50.0), RED, blue.3),
            (blue.0, Vec3::new(0.0, 0.0, 0.0), BLUE, blue.3),
        ],
        None,
    );
    let big = models_of(&turned)[0];
    turned.node_mut(big).unwrap().transform.scale = Vec3::new(2.0, 2.0, 2.0);
    let (counts, batched, each) = both(&gpu, &turned);
    let [r, _, b, _] = batched.pixel(SIZE[0] / 2, SIZE[1] / 2);
    assert!(
        r > b,
        "red, farther and drawn after blue, shows: {r} red, {b} blue"
    );
    assert_within_a_level(&batched, &each, "two alike declared farther first");
    assert_eq!(counts.draw_calls, 2, "{counts:?}");

    // A cube whose inside shows, its front faces culled or its place
    // mirrored, shows the face it has against another: red's inside face
    // and blue's front where they touch are at one depth. Green, nearer and
    // alike blue, is declared first; drawn apart, blue is drawn after red
    // and shows, and a batch of green's would draw it before.
    let inside = |turn: &dyn Fn(&mut Node)| {
        let mut scene = small(
            &[
                (blue.0, Vec3::new(-120.0, 0.0, 150.0), GREEN, blue.3),
                (blue.0, Vec3::new(0.0, 0.0, 50.0), RED, blue.3),
                (blue.0, Vec3::new(0.0, 0.0, -50.0), BLUE, blue.3),
            ],
            None,
        );
        let id = models_of(&scene)[1];
        turn(scene.node_mut(id).unwrap());
        scene
    };
    let front_culled = inside(&|node| match &mut node.kind {
        NodeKind::Model(Model {
            material: Material::Default(m),
            ..
        }) => m.cull_mode = CullMode::Front,
        _ => unreachable!("models"),
    });
    let mirrored = inside(&|node| {
        node.transform.scale = Vec3::new(1.0, 1.0, -1.0);
        match &mut node.kind {
            NodeKind::Model(Model {
                material: Material::Default(m),
                ..
            }) => m.specular_roughness = 0.5,
            _ => unreachable!("models"),
        }
    });
    for (scene, what) in [(front_culled, "front faces culled"), (mirrored, "mirrored")] {
        let (counts, batched, each) = both(&gpu, &scene);
        let [r, _, b, _] = batched.pixel(SIZE[0] / 2, SIZE[1] / 2);
        assert!(
            b > r,
            "{what}: blue, drawn after red's inside, shows: {r} red, {b} blue"
        );
        assert_within_a_level(&batched, &each, what);
        assert_eq!(counts.draw_calls, 3, "{what}: {counts:?}");
    }

    // A batched model's colour is not its shadow's: changing it draws no
    // shadow map again.
    let shadowed = |color: Color| {
        small(
            &[
                (
                    Primitive::Cube,
                    Vec3::new(-120.0, 0.0, 0.0),
                    color,
                    DepthDraw::OpaqueOnly,
                ),
                (
                    Primitive::Cube,
                    Vec3::new(120.0, 0.0, 0.0),
                    RED,
                    DepthDraw::OpaqueOnly,
                ),
            ],
            Some(Shadow::default()),
        )
    };
    let mut lit = Target::new(gpu.device(), &gpu.renderer(), SIZE);
    let (first, _) = frame(&gpu, &mut lit, &shadowed(RED));
    let (again, _) = frame(&gpu, &mut lit, &shadowed(RED));
    assert!(
        first.render_passes > again.render_passes,
        "the first frame draws the shadow map: {first:?} {again:?}"
    );
    let (recoloured, _) = frame(&gpu, &mut lit, &shadowed(BLUE));
    assert_eq!(recoloured.draw_calls, 1, "{recoloured:?}");
    assert_eq!(
        recoloured.render_passes, again.render_passes,
        "a colour draws no shadow: {recoloured:?}"
    );
}

const RED: Color = Color::rgb(1.0, 0.0, 0.0);
const GREEN: Color = Color::rgb(0.0, 1.0, 0.0);
const BLUE: Color = Color::rgb(0.0, 0.0, 1.0);

/// A few models seen from 300 in front, under a light casting `shadow`.
fn small(models: &[(Primitive, Vec3, Color, DepthDraw)], shadow: Option<Shadow>) -> Scene {
    let mut scene = Scene::new();
    let camera = scene.add(
        None,
        Node::new(NodeKind::Camera(Camera::PERSPECTIVE)).with_transform(Transform {
            position: Vec3::new(0.0, 0.0, 300.0),
            ..Transform::default()
        }),
    );
    scene.set_camera(Some(camera));
    let mut light = Node::new(NodeKind::Light(Light::DIRECTIONAL)).with_transform(Transform {
        rotation: Quat::from_euler_degrees(Vec3::new(-30.0, -40.0, 0.0)),
        ..Transform::default()
    });
    light.shadow = shadow;
    scene.add(None, light);
    for &(primitive, position, diffuse_color, depth_draw) in models {
        scene.add(
            None,
            Node::new(NodeKind::Model(Model {
                mesh: MeshSource::Primitive(primitive),
                material: Material::Default(DefaultMaterial {
                    diffuse_color,
                    depth_draw,
                    ..DefaultMaterial::default()
                }),
            }))
            .with_transform(Transform {
                position,
                ..Transform::default()
            }),
        );
    }
    scene
}

/// The models of `scene`, in declaration order.
fn models_of(scene: &Scene) -> Vec<NodeId> {
    scene
        .walk()
        .into_iter()
        .filter(|(_, node, _)| matches!(node.kind, NodeKind::Model(_)))
        .map(|(id, _, _)| id)
        .collect()
}

/// `scene` drawn batched and drawn each on its own, each into a target of
/// its own: the batched frame's counts and both pictures.
fn both(gpu: &Gpu, scene: &Scene) -> (stats::FrameCounters, Rgba, Rgba) {
    let mut target = Target::new(gpu.device(), &gpu.renderer(), SIZE);
    let (counts, batched) = frame(gpu, &mut target, scene);
    gpu.renderer().set_batching(false);
    let mut target = Target::new(gpu.device(), &gpu.renderer(), SIZE);
    let each = frame(gpu, &mut target, scene).1;
    gpu.renderer().set_batching(true);
    (counts, batched, each)
}

fn assert_within_a_level(a: &Rgba, b: &Rgba, what: &str) {
    let far: Vec<_> = a
        .pixels
        .chunks(4)
        .zip(b.pixels.chunks(4))
        .enumerate()
        .filter(|(_, (a, b))| a.iter().zip(*b).any(|(a, b)| a.abs_diff(*b) > 1))
        .map(|(i, (a, b))| (i % SIZE[0] as usize, i / SIZE[0] as usize, a, b))
        .take(8)
        .collect();
    assert!(
        far.is_empty(),
        "{what}: pixels more than a level apart: {far:?}"
    );
}

/// Two cubes in one place, red made first and biased to the far side,
/// blue biased to the near: unbatched, red is drawn after blue and wins
/// the depth test's tie.
fn two_in_one_place() -> Scene {
    let mut scene = Scene::new();
    let camera = scene.add(
        None,
        Node::new(NodeKind::Camera(Camera::PERSPECTIVE)).with_transform(Transform {
            position: Vec3::new(0.0, 0.0, 300.0),
            ..Transform::default()
        }),
    );
    scene.set_camera(Some(camera));
    scene.add(None, Node::new(NodeKind::Light(Light::DIRECTIONAL)));
    let cube = |scene: &mut Scene, color: Color, bias: f32| {
        let mut node = Node::new(NodeKind::Model(Model {
            mesh: MeshSource::Primitive(Primitive::Cube),
            material: Material::Default(DefaultMaterial {
                diffuse_color: color,
                ..DefaultMaterial::default()
            }),
        }))
        .with_transform(Transform {
            rotation: Quat::from_euler_degrees(Vec3::new(20.0, 30.0, 0.0)),
            ..Transform::default()
        });
        node.depth_bias = bias;
        scene.add(None, node);
    };
    cube(&mut scene, Color::rgb(1.0, 0.0, 0.0), 10.0);
    cube(&mut scene, Color::rgb(0.0, 0.0, 1.0), -10.0);
    scene
}
