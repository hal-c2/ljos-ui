//! A 3D view drawn on egui-wgpu's own device, in egui's own pass, over
//! what egui drew under it.

use std::sync::Arc;

use egui_kittest::Harness;
use egui_kittest::wgpu::{WgpuTestRenderer, create_render_state, default_wgpu_setup};
use ljos_3d::math::{Quat, Vec3};
use ljos_3d::offscreen::Offscreen;
use ljos_3d::{
    Background, Camera, Color, DefaultMaterial, Environment, Light, Material, MeshSource, Model,
    Node, NodeKind, Primitive, Scene, Transform,
};
use ljos_egui::scene3d::{Scene3dPaint, Scene3dResources};

const RED: [u8; 4] = [200, 0, 0, 255];

/// `View3D { PerspectiveCamera { z: 300 } DirectionalLight { eulerRotation:
/// Qt.vector3d(-30, -30, 0) } Model { source: "#Cube"; eulerRotation:
/// Qt.vector3d(30, 45, 0); materials: DefaultMaterial { diffuseColor } } }`,
/// with the default, transparent, background.
fn lit_cube(diffuse_color: Color) -> Scene {
    let mut scene = Scene::new();
    scene.set_environment(Environment {
        background: Background::Transparent,
        ..Environment::default()
    });
    scene.add(
        None,
        Node::new(NodeKind::Camera(Camera::PERSPECTIVE)).with_transform(Transform {
            position: Vec3::new(0.0, 0.0, 300.0),
            ..Transform::default()
        }),
    );
    scene.add(
        None,
        Node::new(NodeKind::Light(Light::DIRECTIONAL)).with_transform(Transform {
            rotation: Quat::from_euler_degrees(Vec3::new(-30.0, -30.0, 0.0)),
            ..Transform::default()
        }),
    );
    scene.add(
        None,
        Node::new(NodeKind::Model(Model {
            mesh: MeshSource::Primitive(Primitive::Cube),
            material: Material::Default(DefaultMaterial {
                diffuse_color,
                ..DefaultMaterial::default()
            }),
        }))
        .with_transform(Transform {
            rotation: Quat::from_euler_degrees(Vec3::new(30.0, 45.0, 0.0)),
            ..Transform::default()
        }),
    );
    scene
}

fn blue() -> Color {
    Color::rgb(0x40 as f32 / 255.0, 0x80 as f32 / 255.0, 1.0)
}

/// egui's renderer for tests, with the 3D resources on it; `None`, said,
/// where the machine has no adapter at all.
fn render_state() -> Option<egui_wgpu::RenderState> {
    let state = std::panic::catch_unwind(|| {
        create_render_state(
            default_wgpu_setup(),
            egui_wgpu::RendererOptions::PREDICTABLE,
        )
    });
    match state {
        Ok(state) => {
            Scene3dResources::install(&state);
            Some(state)
        }
        Err(_) => {
            eprintln!("SKIPPED: no wgpu adapter for egui's test renderer");
            None
        }
    }
}

/// Paints, in egui, a red rectangle over the whole screen and then each
/// view over its rect; `views` are (key, scene, rect in points).
fn harness(
    state: &egui_wgpu::RenderState,
    size: egui::Vec2,
    views: Vec<(u64, Arc<Scene>, egui::Rect)>,
) -> Harness<'static> {
    Harness::builder()
        .with_size(size)
        .renderer(WgpuTestRenderer::from_render_state(state.clone()))
        .build_ui(move |ui| {
            let ctx = ui.ctx().clone();
            let ppp = ctx.pixels_per_point();
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::unique("scene3d"),
            ));
            painter.rect_filled(
                ctx.content_rect(),
                0.0,
                egui::Color32::from_rgba_premultiplied(RED[0], RED[1], RED[2], RED[3]),
            );
            for (view, scene, rect) in &views {
                let size_px = [
                    (rect.width() * ppp).round() as u32,
                    (rect.height() * ppp).round() as u32,
                ];
                painter.add(
                    Scene3dPaint {
                        view: *view,
                        scene: Arc::clone(scene),
                        size_px,
                    }
                    .shape(*rect),
                );
            }
        })
}

#[test]
fn a_view_draws_on_eguis_device_over_what_egui_drew() {
    let Some(state) = render_state() else { return };
    let info = state.adapter.get_info();
    let ctx = format!(
        "on {:?} ({:?}), {:?}",
        info.name, info.backend, state.target_format
    );
    let scene = Arc::new(lit_cube(blue()));
    let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(320.0, 240.0));
    let mut h = harness(
        &state,
        egui::vec2(320.0, 240.0),
        vec![(1, Arc::clone(&scene), rect)],
    );
    h.run();
    let image = h.render().expect("a picture");
    let pixel = |x: u32, y: u32| image.get_pixel(x, y).0;
    // For looking at, when a change to the seam needs a picture to argue
    // from; nothing compares against it.
    if let Some(path) = std::env::var_os("LJOS_SCENE3D_PICTURE") {
        let file = std::io::BufWriter::new(std::fs::File::create(path).expect("the file"));
        let mut png = png::Encoder::new(file, image.width(), image.height());
        png.set_color(png::ColorType::Rgba);
        png.set_depth(png::BitDepth::Eight);
        png.write_header()
            .and_then(|mut w| w.write_image_data(image.as_raw()))
            .expect("the picture is written");
    }

    // Where the cube is not, the view is transparent and egui's red shows.
    assert_eq!(pixel(2, 2), RED, "{ctx}");
    assert_eq!(pixel(317, 237), RED, "{ctx}");

    // Where it is, the pixels are the cube's as the renderer draws it on a
    // device of its own: the shared device and the blit change nothing.
    let mut alone = match Offscreen::new() {
        Ok(o) => o,
        Err(e) => panic!("egui had an adapter and the offscreen renderer none: {e}"),
    };
    let reference = alone.render(&scene, 320, 240).unwrap();
    let mut cube = 0;
    for y in 0..240 {
        for x in 0..320 {
            let r = reference.pixel(x, y);
            if r[3] == 255 {
                cube += 1;
                assert_eq!(pixel(x, y), r, "({x}, {y}) {ctx}");
            }
        }
    }
    assert!(cube > 2000, "the cube covers {cube} pixels {ctx}");

    let resources = state.renderer.read();
    let r = resources
        .callback_resources
        .get::<Scene3dResources>()
        .unwrap();
    assert_eq!(r.views(), 1);
}

#[test]
fn two_views_in_one_frame_keep_their_own_scenes() {
    let Some(state) = render_state() else { return };
    let left = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(200.0, 200.0));
    let right = egui::Rect::from_min_size(egui::pos2(200.0, 0.0), egui::vec2(200.0, 200.0));
    let green = Color::rgb(0.2, 0.9, 0.3);
    let mut h = harness(
        &state,
        egui::vec2(400.0, 200.0),
        vec![
            (1, Arc::new(lit_cube(blue())), left),
            (2, Arc::new(lit_cube(green)), right),
        ],
    );
    h.run();
    let image = h.render().expect("a picture");
    let pixel = |x: u32, y: u32| image.get_pixel(x, y).0;
    // The face that faces the light, in each view.
    let l = pixel(100 - 30, 100 - 10);
    let r = pixel(300 - 30, 100 - 10);
    assert!(l[2] > l[1] && l[2] > l[0], "the left cube is blue: {l:?}");
    assert!(r[1] > r[2] && r[1] > r[0], "the right cube is green: {r:?}");

    Scene3dResources::retain(&state, |k| k == 2);
    let resources = state.renderer.read();
    assert_eq!(
        resources
            .callback_resources
            .get::<Scene3dResources>()
            .unwrap()
            .views(),
        1
    );
}

/// The textures egui's device holds for its users: wgpu counts each one
/// handed out and not dropped yet, on every backend, where an allocator's
/// report exists only on Vulkan and DirectX 12.
fn live_textures(state: &egui_wgpu::RenderState) -> usize {
    state
        .instance
        .generate_report()
        .map_or(0, |r| r.hub_report().textures.num_kept_from_user)
}

/// Views that come and go, as a host's do when a `Loader` toggles one:
/// each frame the host lets go of the pictures of the views it no longer
/// paints (`retain`), so after any number of rounds the pictures are those
/// of the views shown, and none once every view is gone.
#[test]
fn views_that_come_and_go_leave_no_pictures_behind() {
    let Some(state) = render_state() else { return };
    let shown: std::rc::Rc<std::cell::RefCell<Vec<u64>>> = Default::default();
    let scene = Arc::new(lit_cube(blue()));
    let painted = std::rc::Rc::clone(&shown);
    let mut h = Harness::builder()
        .with_size(egui::vec2(300.0, 100.0))
        .renderer(WgpuTestRenderer::from_render_state(state.clone()))
        .build_ui(move |ui| {
            let painter = ui.ctx().layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::new("scene3d"),
            ));
            for &view in painted.borrow().iter() {
                let x = (view % 3) as f32 * 100.0;
                let rect = egui::Rect::from_min_size(egui::pos2(x, 0.0), egui::vec2(100.0, 100.0));
                painter.add(
                    Scene3dPaint {
                        view,
                        scene: Arc::clone(&scene),
                        // A size of its own for each round, as a view
                        // laid out again has: a picture of a new size
                        // replaces the old.
                        size_px: [100 + (view % 7) as u32, 100],
                    }
                    .shape(rect),
                );
            }
        });
    let frame = |h: &mut Harness<'_>, views: &[u64]| {
        *shown.borrow_mut() = views.to_vec();
        // What the shell does before each frame (`paint_views`).
        Scene3dResources::retain(&state, |k| views.contains(&k));
        h.run();
        h.render().expect("a picture");
        let resources = state.renderer.read();
        resources
            .callback_resources
            .get::<Scene3dResources>()
            .unwrap()
            .views()
    };
    let none = || live_textures(&state);
    assert_eq!(frame(&mut h, &[]), 0);
    let empty = none();
    // The renderer makes what it shares between views (the white a
    // material without a map reads) when it first draws, and keeps it:
    // the level to come back to is after one view came and went.
    assert_eq!(frame(&mut h, &[u64::MAX]), 1);
    assert_eq!(frame(&mut h, &[]), 0);
    let before = none();
    let mut with_two = None;
    // One run of fifty rounds: views appear, two at a time, with keys
    // never seen before, and go the next round.
    for round in 0..50u64 {
        let views = [2 * round + 1, 2 * round + 2];
        assert_eq!(frame(&mut h, &views), 2, "round {round}: both views drawn");
        let textures = none();
        assert_eq!(
            *with_two.get_or_insert(textures),
            textures,
            "round {round}: two views hold as many textures as two views did in the first round"
        );
        assert_eq!(frame(&mut h, &[]), 0, "round {round}: the views are gone");
        assert_eq!(none(), before, "round {round}: their textures are gone too");
    }
    assert!(
        with_two.is_some_and(|t| t > before) && before >= empty,
        "a view holds textures of its own: {with_two:?} against {before} ({empty} before any view)"
    );
}
