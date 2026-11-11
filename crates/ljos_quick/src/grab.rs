//! `Item.grabToImage` and `ItemGrabResult` (QtQuick).
//!
//! A grab is only asked for when it is called: the scene that shows the
//! item takes it at its next frame, which the ask wakes. The item is drawn
//! there in its own coordinates and cut to its bounds, as that frame left
//! it, with its own opacity and visibility not counted. The picture goes
//! to the callback after the frame, between turns, as an `ItemGrabResult`
//! that scripts own: its `url` names the picture for an image source for
//! as long as the result lives. Nothing is drawn while nothing is asked.

use std::cell::{Cell, RefCell};

use ljos_engine::{
    Atom, Cx, Engine, EngineError, LogLevel, ObjectId, QmlObject, QtObject, Url, Value, qml_impl,
};
use ljos_render::{ImageHandle, RgbaImage};
use rustc_hash::FxHashMap;

use crate::image::ImageStore;

const CATEGORY: &str = "qml";

/// `ItemGrabResult` (QtQuick): what `Item.grabToImage` calls back with.
///
/// `image` holds the same URL as `url`: Ljós has no image value for a
/// script to hold. Rust reads the pixels with [`grabbed_image`].
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQuick",
    name = "ItemGrabResult",
    extends = QtObject,
    uncreatable
)]
pub struct ItemGrabResult {
    #[qml(base)]
    pub object: QtObject,
    #[qml(readonly)]
    pub image: Value,
    #[qml(readonly)]
    pub url: Url,
    #[qml(skip)]
    picture: Option<ImageHandle>,
}

#[qml_impl]
impl ItemGrabResult {
    /// The picture is there: the callback is called after it.
    #[qml(signal)]
    fn ready();

    /// Writes the picture to `fileName`, a path or a `file:` URL, as the
    /// type its suffix names, at once. Only PNG is written; any other
    /// type, or a file that cannot be written, gives false.
    #[qml(invokable, name = "saveToFile")]
    fn save_to_file(cx: &Cx, file_name: Value) -> bool {
        let Some(picture) = grabbed_image(cx.engine, &Value::Object(cx.this)) else {
            return false;
        };
        let name = file_name.to_js_string().to_string();
        // A sandboxed run's `/` is a directory of its own.
        let Some(path) = ljos_platform::Platform::of(cx.engine).file_of(&name) else {
            return false;
        };
        let png = path
            .extension()
            .is_some_and(|x| x.eq_ignore_ascii_case("png"));
        if !png {
            return false;
        }
        let Ok(bytes) = straight(&picture).to_png() else {
            return false;
        };
        std::fs::write(&path, bytes).is_ok()
    }
}

/// The pixels of an `ItemGrabResult`, as many to a logical pixel as the
/// scene that drew them has.
pub fn grabbed_image(engine: &Engine, result: &Value) -> Option<ImageHandle> {
    let Value::Object(obj) = result else {
        return None;
    };
    engine
        .with_mut(*obj, |r: &mut ItemGrabResult| r.picture.clone())
        .ok()
        .flatten()
}

/// A grab asked for and not yet taken.
pub(crate) struct Grab {
    pub(crate) item: ObjectId,
    callback: Value,
    /// The item's size when it was asked, rounded, in logical pixels.
    pub(crate) size: (f64, f64),
    /// The picture's size in logical pixels: `size` unless the caller gave
    /// another.
    pub(crate) target: (f64, f64),
}

#[derive(Default)]
struct Grabs {
    pending: RefCell<Vec<Grab>>,
    /// Numbers the results' URLs.
    made: Cell<u64>,
    /// The URL each live result registered its picture under.
    urls: RefCell<FxHashMap<ObjectId, String>>,
    listening: Cell<bool>,
}

/// `item.grabToImage(callback, targetSize)`: false, with Qt's warning,
/// for a grab the item cannot give; otherwise asks the item's scene for a
/// frame and returns true.
pub(crate) fn grab_to_image(
    engine: &Engine,
    item: ObjectId,
    callback: Value,
    target_size: Value,
) -> Result<bool, EngineError> {
    let refuse = |why: &str| {
        let at = engine.declared_at(item).unwrap_or_default();
        let ty = engine.warning_type_name(item, &native_name(engine, item));
        engine.log(
            LogLevel::Warning,
            CATEGORY,
            format!("{at}QML {ty}: grabToImage: {why}"),
        );
        Ok(false)
    };
    if !engine.is_callable(&callback) {
        return refuse("'callback' is not a function");
    }
    let Some(window) = engine.untrack(|| crate::layer::window_of(engine, item)) else {
        return refuse("item is not attached to a window");
    };
    // A Window that is not shown has `visibility` Hidden (0); a scene's
    // plain root item has none.
    if engine.find_property(window, "visibility").is_some()
        && engine.untrack(|| engine.read_by_name(window, "visibility").to_number()) == 0.0
    {
        return refuse("item's window is not visible");
    }
    let target = match &target_size {
        Value::Undefined | Value::Null => None,
        Value::Size(s) => Some((s.width, s.height)),
        _ => {
            return Err(EngineError::Js(
                "TypeError: Passing incompatible arguments to C++ functions from JavaScript is not allowed."
                    .into(),
            ));
        }
    };
    let size = engine.untrack(|| {
        (
            engine
                .read(item, crate::Item::WIDTH.index)
                .to_number()
                .round(),
            engine
                .read(item, crate::Item::HEIGHT.index)
                .to_number()
                .round(),
        )
    });
    // A target of no size is no target: the item's own size is taken.
    let target = target
        .map(|(w, h)| (w.round(), h.round()))
        .filter(|(w, h)| *w > 0.0 && *h > 0.0)
        .unwrap_or(size);
    if !(target.0 >= 1.0 && target.1 >= 1.0) {
        return refuse(&format!(
            "item's width and/or height are less than 1: QSize({}, {})",
            target.0, target.1
        ));
    }
    let grabs = engine.extension::<Grabs>();
    if !grabs.listening.replace(true) {
        engine.on_destroyed(forget_results);
    }
    grabs.pending.borrow_mut().push(Grab {
        item,
        callback,
        size,
        target,
    });
    crate::scene::pictures_wanted(engine);
    Ok(true)
}

/// The type of `item` its warnings name when it was declared in no file:
/// its first native type.
fn native_name(engine: &Engine, item: ObjectId) -> String {
    let mut meta = engine.meta_of(item);
    while let Some(m) = meta {
        if m.declared.is_none() {
            return m.name.to_string();
        }
        meta = m.base.clone();
    }
    "Item".to_owned()
}

/// Hands a scene the grabs of the items `shows` says it shows. A grab of
/// an item that is gone is dropped.
pub(crate) fn take(engine: &Engine, shows: impl Fn(ObjectId) -> bool) -> Vec<Grab> {
    let grabs = engine.extension::<Grabs>();
    let mut pending = grabs.pending.borrow_mut();
    if pending.is_empty() {
        return Vec::new();
    }
    pending.retain(|g| engine.is_alive(g.item));
    let (taken, left) = std::mem::take(&mut *pending)
        .into_iter()
        .partition(|g| shows(g.item));
    *pending = left;
    taken
}

/// Whether a grab waits for a scene to take it.
pub(crate) fn waiting(engine: &Engine) -> bool {
    !engine.extension::<Grabs>().pending.borrow().is_empty()
}

/// A scene drew `grab` as `picture`: the result is made and the callback
/// called once the frame is over.
pub(crate) fn taken(engine: &Engine, grab: Grab, picture: ImageHandle) {
    engine.call_at(engine.now(), move |e| deliver(e, grab, picture));
}

fn deliver(engine: &Engine, grab: Grab, picture: ImageHandle) {
    let grabs = engine.extension::<Grabs>();
    let n = grabs.made.get() + 1;
    grabs.made.set(n);
    let url = format!("itemgrabber:#{n}");
    match straight(&picture).to_png() {
        Ok(bytes) => ImageStore::of(engine).register_resource(url.clone(), bytes),
        Err(err) => {
            engine.log(
                LogLevel::Warning,
                CATEGORY,
                format!("grabToImage: cannot encode the picture: {err}"),
            );
            return;
        }
    }
    let atom = Atom::new(&url);
    let native = ItemGrabResult {
        object: QtObject::default(),
        image: Value::Url(atom.clone()),
        url: Url(atom),
        picture: Some(picture),
    };
    let Ok(result) = engine.create_native_incomplete(native, None) else {
        ImageStore::of(engine).remove_resource(&url);
        return;
    };
    // Before the result first crosses to a script: the scripts holding it
    // decide how long it lives, as they do in Qt.
    engine.give_to_script(result);
    grabs.urls.borrow_mut().insert(result, url);
    if engine.complete(result).is_err() {
        return;
    }
    let _ = engine.emit_by_name(result, "ready", &[]);
    if engine.is_alive(result) {
        let _ = engine.call_js(&grab.callback, &[Value::Object(result)]);
    }
}

/// A result that is gone takes its picture's URL with it.
fn forget_results(engine: &Engine, gone: &[ObjectId]) {
    let grabs = engine.extension::<Grabs>();
    let mut urls = grabs.urls.borrow_mut();
    if urls.is_empty() {
        return;
    }
    let store = ImageStore::of(engine);
    for obj in gone {
        if let Some(url) = urls.remove(obj) {
            store.remove_resource(&url);
        }
    }
}

/// The picture with straight alpha, as a file holds it.
fn straight(picture: &ImageHandle) -> RgbaImage {
    let mut data = picture.data().to_vec();
    for p in data.as_chunks_mut::<4>().0 {
        let a = u32::from(p[3]);
        if a != 0 && a != 255 {
            for c in &mut p[..3] {
                *c = ((u32::from(*c) * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
    RgbaImage {
        width: picture.width(),
        height: picture.height(),
        data,
    }
}

pub(crate) fn register(engine: &Engine) {
    engine.register_module(ljos_engine::module!("QtQuick" 2.15 => [ItemGrabResult]));
}
