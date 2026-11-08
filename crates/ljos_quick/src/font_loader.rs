//! `FontLoader` (QtQuick), `Qt.fontFamilies()`, and the set of fonts as
//! something a binding follows.
//!
//! A loaded file's faces go into the text engine's collection, so its
//! family is usable by name in any text from then on. A local file is read
//! on the spot, as Qt does; a remote one is fetched off the Engine thread
//! (`ljos_platform`), and the loader says `Loading` until it arrives.

use std::cell::Cell;
use std::sync::Arc;

use ljos_engine::{
    Atom, Cx, Engine, EngineError, Font, LogLevel, ObjectId, PropIndex, QmlObject, QtObject, Url,
    Value, qml_impl,
};
use ljos_platform::{FetchError, Platform, is_remote};

use crate::enums::{EnumTable, loader_status as status};
use crate::item::write_computed;
use crate::text::{font_families, text_engine};

const CATEGORY: &str = "qml";

pub(crate) fn font_loader_enums() -> EnumTable {
    vec![(
        "Status",
        vec![
            ("Null", i64::from(status::NULL)),
            ("Ready", i64::from(status::READY)),
            ("Loading", i64::from(status::LOADING)),
            ("Error", i64::from(status::ERROR)),
        ],
    )]
}

/// `FontLoader` (QtQuick).
///
/// Clearing `source` makes the loader `Null` again; Qt 6.11 says `Loading`
/// then, which nothing documents.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick",
    name = "FontLoader",
    extends = QtObject,
    enums = font_loader_enums
)]
pub struct FontLoader {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify)]
    pub source: Url,
    /// The family of the first face in the file.
    #[qml(notify, readonly)]
    pub name: Atom,
    /// [`crate::enums::loader_status`].
    #[qml(notify, readonly, enumeration = "Status")]
    pub status: i32,
    /// The first face: its family at its own weight and slant.
    #[qml(notify, readonly)]
    pub font: Font,
    /// Bumped by every load, so that an answer to an older one is dropped.
    #[qml(skip)]
    generation: u64,
}

impl Default for FontLoader {
    fn default() -> Self {
        FontLoader {
            object: QtObject::default(),
            source: Url::default(),
            name: Atom::EMPTY,
            status: status::NULL,
            font: Font::default(),
            generation: 0,
        }
    }
}

#[qml_impl]
impl FontLoader {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        e.watch(me, FontLoader::SOURCE.index, move |e, _| load(e, me))?;
        load(e, me);
        Ok(())
    }
}

fn set(e: &Engine, me: ObjectId, p: PropIndex, v: Value) {
    if e.is_alive(me)
        && let Err(err) = write_computed(e, me, p, v)
    {
        e.log(LogLevel::Warning, CATEGORY, format!("{err}"));
    }
}

fn load(e: &Engine, me: ObjectId) {
    let Ok(generation) = e.with_mut(me, |f: &mut FontLoader| {
        f.generation += 1;
        f.generation
    }) else {
        return;
    };
    let source = e.untrack(|| e.read(me, FontLoader::SOURCE.index).to_js_string());
    if source.is_empty() {
        e.batch(|| {
            set(e, me, FontLoader::NAME.index, Value::String(Atom::EMPTY));
            set(
                e,
                me,
                FontLoader::FONT.index,
                Value::Font(Arc::new(Font::default())),
            );
            set(e, me, FontLoader::STATUS.index, Value::Int(status::NULL));
        });
        return;
    }
    let url = e.resolve_url(me, &source);
    let platform = Platform::of(e);
    if let Some(bytes) = platform.resource(&url) {
        loaded(e, me, &url, Some(bytes.to_vec()));
        return;
    }
    if is_remote(&url) {
        set(e, me, FontLoader::STATUS.index, Value::Int(status::LOADING));
        let asked = url.clone();
        platform.fetch_for(
            Some(me),
            &url,
            move |e, got: Result<Vec<u8>, FetchError>| {
                let current = e
                    .with_mut(me, |f: &mut FontLoader| f.generation == generation)
                    .unwrap_or(false);
                if current {
                    loaded(e, me, &asked, got.ok());
                }
            },
        );
        return;
    }
    // A sandboxed run (a test) keeps every file under its root.
    let bytes = platform.file_of(&url).and_then(|p| std::fs::read(p).ok());
    loaded(e, me, &url, bytes);
}

/// Registers what was read, or says why there is nothing.
fn loaded(e: &Engine, me: ObjectId, url: &str, bytes: Option<Vec<u8>>) {
    let faces = bytes.and_then(|b| text_engine().borrow_mut().register_font_faces(b).ok());
    let Some(face) = faces.and_then(|f| f.into_iter().next()) else {
        e.log(
            LogLevel::Warning,
            CATEGORY,
            format!(
                "QML FontLoader: Cannot load font: \"{}\"",
                ljos_engine::diagnostic_url(url)
            ),
        );
        e.batch(|| {
            set(e, me, FontLoader::NAME.index, Value::String(Atom::EMPTY));
            set(e, me, FontLoader::STATUS.index, Value::Int(status::ERROR));
        });
        return;
    };
    let family = Atom::from(face.family.as_str());
    let font = Font {
        family: family.clone(),
        weight: face.weight.round() as i32,
        italic: face.italic,
        ..Font::default()
    };
    // Name and font first: a handler of `status` sees them set.
    e.batch(|| {
        set(e, me, FontLoader::NAME.index, Value::String(family));
        set(e, me, FontLoader::FONT.index, Value::Font(Arc::new(font)));
        set(e, me, FontLoader::STATUS.index, Value::Int(status::READY));
    });
    // At once, not at the next frame: a binding that reads the face a
    // family resolves to after this loader completed sees the font.
    fonts_changed(e);
    e.request_wake();
}

/// Which fonts the text engine has, for what tells of the face a font
/// resolves to (`FontInfo`): its bindings read the generation, so a font
/// registered after them works them out again, as in Qt.
#[derive(Debug, Default, QmlObject)]
#[qml(name = "FontSet", uncreatable)]
pub struct FontSet {
    /// [`ljos_text::TextEngine::generation`] of the UI thread's engine.
    #[qml(notify, readonly)]
    pub generation: f64,
}

/// The engine's [`FontSet`], made by the first binding that follows it.
#[derive(Default)]
struct FontSetOf(Cell<Option<ObjectId>>);

fn generation() -> f64 {
    text_engine().borrow().generation() as f64
}

/// Makes the binding being worked out follow the set of fonts.
pub(crate) fn follow_fonts(e: &Engine) {
    let of = e.extension::<FontSetOf>();
    let set = match of.0.get().filter(|o| e.is_alive(*o)) {
        Some(set) => set,
        None => {
            let generation = generation();
            let Ok(set) = e.create_native_incomplete(FontSet { generation }, None) else {
                return;
            };
            of.0.set(Some(set));
            set
        }
    };
    let _ = e.read(set, FontSet::GENERATION.index);
}

/// Tells what follows the set of fonts that it may have changed; nothing
/// when nothing follows it.
pub(crate) fn fonts_changed(e: &Engine) {
    let Some(set) = e.extension::<FontSetOf>().0.get() else {
        return;
    };
    let generation = generation();
    let known = e.untrack(|| e.read(set, FontSet::GENERATION.index).to_number());
    if known != generation {
        self::set(e, set, FontSet::GENERATION.index, Value::Real(generation));
    }
}

/// What `Qt.fontFamilies()` asks; one per engine, made when scripts start.
#[derive(Debug, Default, QmlObject)]
#[qml(name = "FontFamilies", uncreatable)]
pub struct FontFamilies {}

#[qml_impl]
impl FontFamilies {
    #[qml(invokable, name = "families")]
    fn families(_cx: &Cx) -> Value {
        let names = font_families();
        Value::List(
            names
                .into_iter()
                .map(|n| Value::String(Atom::from(n.as_str())))
                .collect::<Vec<_>>()
                .into(),
        )
    }
}

const INSTALL: &str = r#"(function (helper) {
  Object.defineProperty(Qt, "fontFamilies", {
    value: function () { return Array.from(helper.families()); },
    enumerable: true,
  });
})"#;

pub(crate) fn register(engine: &Engine) {
    engine.on_js_started(|e| {
        let installed = e
            .create_native_incomplete(FontFamilies {}, None)
            .map_err(|err| err.to_string())
            .and_then(|helper| {
                let install = e.eval_js(INSTALL).map_err(|err| err.to_string())?;
                e.call_js(&install, &[Value::Object(helper)]);
                Ok(())
            });
        if let Err(err) = installed {
            e.log(
                LogLevel::Warning,
                crate::scene::SCENE_CATEGORY,
                format!("Qt.fontFamilies is not available: {err}"),
            );
        }
    });
}
