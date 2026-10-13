//! A view whose window is hidden gives back what it can rebuild
//! (ARCHITECTURE §8.5): the renderer's image textures and glyph atlas, and
//! decoded pictures nothing shows. Shown again, it draws the same picture.

use std::rc::Rc;

use egui::{Pos2, Rect, TextureId, Vec2};
use egui_kittest::Harness;
use ljos_egui::QmlView;
use ljos_engine::{Engine, ObjectId, QmlType, Value};
use ljos_quick::{Image, ImageStore, Item, Text, add_data};
use ljos_runtime::MemoryPressure;

/// A `w` by `h` PNG: the left half red, the right half blue.
fn halves(w: u32, h: u32) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().unwrap();
        let data: Vec<u8> = (0..w * h)
            .flat_map(|i| {
                if i % w < w / 2 {
                    [255, 0, 0, 255]
                } else {
                    [0, 0, 255, 255]
                }
            })
            .collect();
        writer.write_image_data(&data).unwrap();
    }
    out
}

fn make<T: QmlType + Default>(e: &Engine, parent: Option<ObjectId>) -> ObjectId {
    let o = e.create_native(T::default(), parent).unwrap();
    if let Some(p) = parent {
        add_data(e, p, o).unwrap();
    }
    e.complete(o).unwrap();
    o
}

fn set(e: &Engine, o: ObjectId, name: &str, v: Value) {
    e.write_by_name(o, name, v)
        .unwrap_or_else(|err| panic!("{name}: {err}"));
}

struct State {
    view: QmlView,
    rect: Rect,
}

/// An 80 by 40 Image at (10, 10), a Text under it, and a second Image
/// that showed a picture and then let it go, so that only the cache has it.
fn scene() -> (Rc<Engine>, Harness<'static, State>) {
    let e = Rc::new(Engine::new());
    ljos_quick::register(&e);
    let store = ImageStore::of(&e);
    store.register_resource("halves.png", halves(4, 2));
    store.register_resource("gone.png", halves(16, 16));
    let root = make::<Item>(&e, None);
    let image = make::<Image>(&e, Some(root));
    e.set(image, Item::X, 10.0).unwrap();
    e.set(image, Item::Y, 10.0).unwrap();
    e.set(image, Item::WIDTH, 80.0).unwrap();
    e.set(image, Item::HEIGHT, 40.0).unwrap();
    set(&e, image, "smooth", Value::Bool(false));
    set(&e, image, "asynchronous", Value::Bool(false));
    set(&e, image, "source", Value::Url("halves.png".into()));
    let gone = make::<Image>(&e, Some(root));
    set(&e, gone, "asynchronous", Value::Bool(false));
    set(&e, gone, "source", Value::Url("gone.png".into()));
    set(&e, gone, "source", Value::Url("".into()));
    let text = make::<Text>(&e, Some(root));
    e.set(text, Item::X, 10.0).unwrap();
    e.set(text, Item::Y, 70.0).unwrap();
    set(&e, text, "text", Value::string("Hello"));
    let view = QmlView::new(Rc::clone(&e), root).unwrap();
    let h = Harness::builder()
        .with_size(Vec2::new(200.0, 120.0))
        .build_ui_state(
            |ui, st: &mut State| st.rect = st.view.show(ui).rect,
            State {
                view,
                rect: Rect::NOTHING,
            },
        );
    (e, h)
}

fn textured_at(h: &Harness<'_, State>, p: Pos2) -> bool {
    h.output().shapes.iter().any(|cs| match &cs.shape {
        egui::Shape::Mesh(m) => {
            matches!(m.texture_id, TextureId::Managed(id) if id != 0)
                && cs.clip_rect.contains(p)
                && m.calc_bounds().contains(p)
        }
        _ => false,
    })
}

fn assert_painted(h: &mut Harness<'_, State>) {
    let origin = h.state().rect.min;
    let left = origin + Vec2::new(20.0, 30.0);
    let right = origin + Vec2::new(80.0, 30.0);
    assert!(textured_at(h, left) && textured_at(h, right));
    match h.render() {
        Ok(img) => {
            let px = |p: Pos2| img.get_pixel(p.x as u32, p.y as u32).0;
            assert_eq!(px(left), [255, 0, 0, 255]);
            assert_eq!(px(right), [0, 0, 255, 255]);
        }
        Err(err) => eprintln!("render unavailable ({err}); checked the shapes only"),
    }
}

#[test]
fn a_hidden_view_gives_back_its_caches_and_draws_again_when_shown() {
    let (e, mut h) = scene();
    let store = ImageStore::of(&e);
    h.run();
    assert_painted(&mut h);
    let backend = h.state().view.backend();
    assert_eq!(backend.image_textures(), 1);
    assert!(backend.glyph_atlas().glyphs > 0);
    let uploads = backend.texture_uploads();
    let glyphs = backend.glyph_atlas().glyphs;
    assert_eq!(store.len(), 2);
    let cached = store.bytes();

    // Hidden: the host stops drawing it and trims it, as the shell does.
    h.state_mut().view.set_drawn(false);
    h.state_mut().view.trim(MemoryPressure::Critical);
    // Its scene still runs while it is hidden, and builds nothing back.
    h.run();
    let backend = h.state().view.backend();
    assert_eq!(backend.image_textures(), 0);
    assert_eq!(backend.glyph_atlas().glyphs, 0);
    let origin = h.state().rect.min;
    assert!(!textured_at(&h, origin + Vec2::new(20.0, 30.0)));
    // The picture on screen stays; the one nothing shows is gone.
    assert_eq!(store.cached_urls(), vec!["halves.png".to_owned()]);
    assert_eq!(cached - store.bytes(), 16 * 16 * 4);

    // Shown again: the same picture, its texture uploaded once more.
    h.state_mut().view.set_drawn(true);
    h.run();
    assert_painted(&mut h);
    let backend = h.state().view.backend();
    assert_eq!(backend.image_textures(), 1);
    assert_eq!(backend.texture_uploads(), uploads + 1);
    assert_eq!(backend.glyph_atlas().glyphs, glyphs);
}

#[test]
fn moderate_pressure_keeps_what_is_on_screen() {
    let (_e, mut h) = scene();
    h.run();
    let uploads = h.state().view.backend().texture_uploads();
    h.state_mut().view.trim(MemoryPressure::Moderate);
    h.run();
    assert_painted(&mut h);
    let backend = h.state().view.backend();
    assert_eq!(backend.image_textures(), 1);
    assert_eq!(backend.texture_uploads(), uploads);
}
