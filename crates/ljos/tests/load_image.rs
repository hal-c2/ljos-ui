//! A painted item that shows a picture by URL: it asks the facade for
//! it, keeps the handle that comes back and paints it. The picture is
//! read and decoded off the UI thread, in the cache `Image` uses. Only
//! the host's part (framing the scene, and waiting for decodes as a
//! test does) reaches below the facade.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use ljos::item::{
    self, DisplayListBuilder, DrawOp, ImageAlign, ImageFit, ImageHandle, ItemPaint, TextEngine,
    kurbo, place_image,
};
use ljos::prelude::*;
use ljos_quick::{ImageStore, QuickScene};
use ljos_runtime::{Clock as _, VirtualClock};

#[derive(Debug, Default, QmlObject)]
#[qml(module = "Example", name = "Photo", extends = Item)]
pub struct Photo {
    #[qml(base)]
    pub item: Item,
    /// How many pictures arrived, read by the paint so that one that
    /// arrives repaints the item.
    #[qml(notify)]
    pub arrived: i32,
}

/// The picture each photo was given.
#[derive(Default)]
struct Pictures(RefCell<HashMap<ObjectId, ImageHandle>>);

#[derive(Debug)]
struct Shown(Option<ImageHandle>);

impl ItemPaint for Shown {
    fn paint(&self, size: kurbo::Size, _: &mut TextEngine, out: &mut DisplayListBuilder) {
        let Some(image) = &self.0 else {
            return;
        };
        let natural = kurbo::Size::new(f64::from(image.width()), f64::from(image.height()));
        let align = [ImageAlign::Center, ImageAlign::Center];
        if let Some(at) = place_image(natural, size, ImageFit::Fit, align, [false, false]) {
            out.image(image.clone(), at.dst);
        }
    }
}

fn read(engine: &Engine, photo: ObjectId) -> Rc<dyn ItemPaint> {
    let _ = item::prop(engine, photo, Photo::ARRIVED);
    let pictures = engine.extension::<Pictures>();
    let image = pictures.0.borrow().get(&photo).cloned();
    Rc::new(Shown(image))
}

/// Asks for the picture at `url` for `photo`; the counter says how many
/// replies came.
fn show(engine: &Engine, photo: ObjectId, url: &str, replies: &Rc<Cell<u32>>) {
    let replies = Rc::clone(replies);
    item::load_image(engine, url, move |engine, picture| {
        replies.set(replies.get() + 1);
        let picture = picture.unwrap();
        let pictures = engine.extension::<Pictures>();
        pictures.0.borrow_mut().insert(photo, picture);
        let n = item::prop(engine, photo, Photo::ARRIVED);
        engine.set(photo, Photo::ARRIVED, n + 1).unwrap();
    });
}

fn images(scene: &QuickScene, photo: ObjectId) -> Vec<(u32, u32, kurbo::Rect)> {
    let tree = scene.render_tree();
    let Some(node) = tree.node(scene.node_of(photo).unwrap()) else {
        return Vec::new();
    };
    let Some(content) = node.content() else {
        return Vec::new();
    };
    content
        .ops()
        .iter()
        .filter_map(|op| match op {
            DrawOp::Image { image, dst, .. } => Some((image.width(), image.height(), *dst)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_painted_item_shows_a_picture_it_loaded_by_url() {
    let engine = ljos::engine();
    let clock = Arc::new(VirtualClock::new());
    engine.set_clock(Rc::new(Arc::clone(&clock)));
    engine.register_module(ljos::module!("Example" 1.0 => [Photo]));
    item::register_paint_item::<Photo>(&engine, read);
    let photo = engine.create_by_name("Example", "Photo", None).unwrap();
    engine
        .write_by_name(photo, "width", Value::from(60.0))
        .unwrap();
    engine
        .write_by_name(photo, "height", Value::from(20.0))
        .unwrap();
    let mut scene = QuickScene::new(kurbo::Size::new(100.0, 100.0));
    scene.set_root(&engine, photo).unwrap();
    scene.frame(&engine, Time::ZERO);
    assert!(images(&scene, photo).is_empty());

    // A 3 by 2 picture, as a file.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("red.png");
    let png = ljos_render::RgbaImage::filled(3, 2, [255, 0, 0, 255]);
    std::fs::write(&path, png.to_png().unwrap()).unwrap();
    let url = file_url(&path);

    let replies = Rc::new(Cell::new(0));
    show(&engine, photo, &url, &replies);
    // The reply comes later, never inside the call.
    assert_eq!(replies.get(), 0);
    ImageStore::of(&engine).finish(&engine);
    assert_eq!(replies.get(), 1);
    scene.frame(&engine, Time::ZERO);
    // Fit to 60 by 20: 30 by 20, in the middle.
    let rect = kurbo::Rect::new(15.0, 0.0, 45.0, 20.0);
    assert_eq!(images(&scene, photo), vec![(3, 2, rect)]);

    // Asked again, it comes from the cache, not decoded again, and still
    // later.
    let decodes = ImageStore::of(&engine).decodes();
    show(&engine, photo, &url, &replies);
    assert_eq!(replies.get(), 1);
    clock.set(clock.now() + Duration::from_millis(1));
    engine.advance_timers(clock.now());
    assert_eq!(replies.get(), 2);
    assert_eq!(ImageStore::of(&engine).decodes(), decodes);
}

#[test]
fn a_picture_that_cannot_be_read_says_why() {
    let engine = ljos::engine();
    let dir = tempfile::tempdir().unwrap();
    let url = file_url(&dir.path().join("missing.png"));
    let said = Rc::new(RefCell::new(None));
    let to = Rc::clone(&said);
    item::load_image(&engine, &url, move |_, picture| {
        *to.borrow_mut() = Some(picture.map(|_| ()));
    });
    ImageStore::of(&engine).finish(&engine);
    let said = said.borrow_mut().take().unwrap();
    assert!(
        matches!(said, Err(item::ImageLoadError::CannotOpen(_))),
        "{said:?}"
    );
}
