//! `RuntimeLoader`: a node whose children come from a glTF file read
//! while the application runs.
//!
//! The file is read and parsed on the compute pool. A `.gltf` that names
//! other files (buffers, images) is parsed first; the main thread then
//! finds those files as it finds any source (a resource, a local path or
//! a fetch) and the pool reads and builds the model from them. A late
//! answer for a source that has since changed is dropped by its
//! generation.

#[cfg(feature = "gltf")]
use std::sync::Arc;

use ljos::item;
#[cfg(feature = "gltf")]
use ljos::item::ljos_3d::gltf::Asset;
use ljos::prelude::*;

use crate::Node;
use crate::space::to_value;

/// `RuntimeLoader`: the nodes, meshes and materials of a glTF 2.0 file
/// (`.gltf` with its files, or `.glb`), as children of this node.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQuick3D",
    name = "RuntimeLoader",
    extends = Node,
    enums = loader_enums
)]
pub struct RuntimeLoader {
    #[qml(base)]
    pub node: Node,
    #[qml(notify)]
    pub source: Url,
    #[qml(notify, readonly)]
    pub status: i32,
    #[qml(notify, readonly)]
    pub error_string: String,
    /// The box the model fills in this node's space: a map of `minimum`
    /// and `maximum`, each a `vector3d`.
    #[qml(notify, readonly)]
    pub bounds: Value,
    /// Kept; the drawing does not use it yet.
    #[qml(notify, object = "Object3D")]
    pub instancing: Option<ObjectId>,
    #[qml(skip)]
    generation: u64,
    #[cfg(feature = "gltf")]
    #[qml(skip)]
    asset: Option<Arc<Asset>>,
}

pub(crate) mod status {
    pub const EMPTY: i32 = 0;
    pub const SUCCESS: i32 = 1;
    pub const ERROR: i32 = 2;
}

fn loader_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![(
        "Status",
        vec![
            ("Empty", status::EMPTY.into()),
            ("Success", status::SUCCESS.into()),
            ("Error", status::ERROR.into()),
        ],
    )]
}

#[qml_impl]
impl RuntimeLoader {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        set_bounds(e, me, None);
        load(e, me);
        e.watch(me, RuntimeLoader::SOURCE.index, move |e, _| load(e, me))?;
        Ok(())
    }
}

/// The model loaded under `obj`, if it is a loader that has one. Tracked:
/// a walk that reads it is walked again when another model arrives.
#[cfg(feature = "gltf")]
pub(crate) fn asset(e: &Engine, obj: ObjectId) -> Option<Arc<Asset>> {
    if !e.is::<RuntimeLoader>(obj) {
        return None;
    }
    // Each arrival notifies `bounds`, even when the box is the same.
    e.read(obj, RuntimeLoader::BOUNDS.index);
    e.with(obj, |l: &RuntimeLoader| l.asset.clone())
        .ok()
        .flatten()
}

fn set_bounds(e: &Engine, me: ObjectId, bounds: Option<(Value, Value)>) {
    let zero = || to_value(ljos::item::ljos_3d::math::Vec3::ZERO);
    let (min, max) = bounds.unwrap_or_else(|| (zero(), zero()));
    let value = Value::map([("minimum", min), ("maximum", max)]);
    if e.with_mut(me, |l: &mut RuntimeLoader| l.bounds = value)
        .is_ok()
        && let Err(err) = e.notify(me, RuntimeLoader::BOUNDS.index)
    {
        e.warn(format!("QtQuick3D RuntimeLoader: {err}"));
    }
}

fn finish(e: &Engine, me: ObjectId, s: i32, error: String) {
    let _ = e.set_internal(me, RuntimeLoader::ERROR_STRING, error);
    let _ = e.set_internal(me, RuntimeLoader::STATUS, s);
    item::repaint(e, me);
}

/// Starts (or restarts) loading `source`, dropping what was loaded.
fn load(e: &Engine, me: ObjectId) {
    let source = e.untrack(|| e.read(me, RuntimeLoader::SOURCE.index).to_js_string());
    let Ok(generation) = e.untrack(|| {
        e.with_mut(me, |l: &mut RuntimeLoader| {
            l.generation += 1;
            #[cfg(feature = "gltf")]
            {
                l.asset = None;
            }
            l.generation
        })
    }) else {
        return;
    };
    set_bounds(e, me, None);
    // Qt has no Loading status: a loader that has not loaded is Empty.
    finish(e, me, status::EMPTY, String::new());
    if source.is_empty() {
        return;
    }
    start(e, me, generation, source.to_string());
}

#[cfg(not(feature = "gltf"))]
fn start(e: &Engine, me: ObjectId, _generation: u64, source: String) {
    finish(
        e,
        me,
        status::ERROR,
        format!("{source}: this build reads no glTF files (the `gltf` feature is off)"),
    );
}

#[cfg(feature = "gltf")]
use gltf_load::start;

#[cfg(feature = "gltf")]
mod gltf_load {
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::Arc;

    use ljos::item::ljos_3d::gltf::{self, Asset, Document};
    use ljos::prelude::*;

    use super::{RuntimeLoader, finish, set_bounds, status};
    use crate::space::to_value;

    const CATEGORY: &str = "qt.quick3d.runtimeloader";

    /// Where one file's bytes are: already had, or a local file the pool
    /// reads.
    enum Found {
        Bytes(Vec<u8>),
        Path(PathBuf),
        Missing(String),
    }

    impl Found {
        fn read(self) -> Result<Vec<u8>, String> {
            match self {
                Found::Bytes(b) => Ok(b),
                Found::Path(p) => std::fs::read(&p).map_err(|err| err.to_string()),
                Found::Missing(why) => Err(why),
            }
        }
    }

    /// What the first pass on the pool carries back.
    enum Parsed {
        Built(Result<Asset, String>),
        /// The document, and the files it names that must be found first.
        Needs(Box<Document>, Vec<String>),
    }

    pub(super) fn start(e: &Engine, me: ObjectId, generation: u64, source: String) {
        let url = e.resolve_url(me, &source);
        let platform = Platform::of(e);
        let found = if let Some(bytes) = platform.resource(&url) {
            Found::Bytes(bytes.to_vec())
        } else if is_remote(&url) {
            let base = url.clone();
            platform.fetch_for(Some(me), &url, move |e, got| {
                let found = got.map_or_else(|err| Found::Missing(err.to_string()), Found::Bytes);
                parse(e, me, generation, source, base, found);
            });
            return;
        } else if let Some(path) = url_path(&url).and_then(|p| platform.env().real(&p).ok()) {
            Found::Path(path)
        } else {
            Found::Missing("no such file".to_owned())
        };
        parse(e, me, generation, source, url, found);
    }

    fn parse(
        e: &Engine,
        me: ObjectId,
        generation: u64,
        source: String,
        base: String,
        found: Found,
    ) {
        let work = move || -> Result<Parsed, String> {
            let bytes = found.read()?;
            let doc = gltf::parse(&bytes).map_err(|err| err.to_string())?;
            let uris = doc.external_uris();
            if uris.is_empty() {
                let mut none = |uri: &str| Err(format!("{uri} is not in the file"));
                Ok(Parsed::Built(
                    doc.build(&mut none).map_err(|e| e.to_string()),
                ))
            } else {
                Ok(Parsed::Needs(Box::new(doc), uris))
            }
        };
        let spawned = ljos::task::spawn_for(
            e,
            format!("glTF {source}"),
            Some(me),
            work,
            move |e, got| match flatten(got) {
                Ok(Parsed::Built(built)) => arrived(e, me, generation, &source, built),
                Ok(Parsed::Needs(doc, uris)) => {
                    if current(e, me, generation) {
                        gather(e, me, generation, source, base, doc, uris, HashMap::new());
                    }
                }
                Err(why) => arrived(e, me, generation, &source, Err(why)),
            },
        );
        if spawned.is_err() {
            refuse_pool_full(e, me, generation);
        }
    }

    fn flatten<T>(got: Result<Result<T, String>, ljos::task::TaskPanic>) -> Result<T, String> {
        got.unwrap_or_else(|_| Err("reading the file failed".to_owned()))
    }

    fn refuse_pool_full(e: &Engine, me: ObjectId, generation: u64) {
        if current(e, me, generation) {
            finish(e, me, status::ERROR, "too much work is waiting".to_owned());
        }
    }

    /// Finds the files the document names one at a time (a remote one
    /// answers later), then builds the model on the pool.
    #[allow(clippy::too_many_arguments)]
    fn gather(
        e: &Engine,
        me: ObjectId,
        generation: u64,
        source: String,
        base: String,
        doc: Box<Document>,
        mut rest: Vec<String>,
        mut found: HashMap<String, Found>,
    ) {
        let platform = Platform::of(e);
        while let Some(uri) = rest.pop() {
            let url = join(&base, &uri);
            if let Some(bytes) = platform.resource(&url) {
                found.insert(uri, Found::Bytes(bytes.to_vec()));
            } else if is_remote(&url) {
                platform.fetch_for(Some(me), &url, move |e, got| {
                    if !current(e, me, generation) {
                        return;
                    }
                    let f = got.map_or_else(|err| Found::Missing(err.to_string()), Found::Bytes);
                    found.insert(uri, f);
                    gather(e, me, generation, source, base, doc, rest, found);
                });
                return;
            } else if let Some(path) = url_path(&url).and_then(|p| platform.env().real(&p).ok()) {
                found.insert(uri, Found::Path(path));
            } else {
                found.insert(uri, Found::Missing("no such file".to_owned()));
            }
        }
        let work = move || -> Result<Asset, String> {
            let mut fetch = |uri: &str| {
                found
                    .remove(uri)
                    .map_or_else(|| Err("not named".to_owned()), Found::read)
            };
            doc.build(&mut fetch).map_err(|err| err.to_string())
        };
        let spawned = ljos::task::spawn_for(
            e,
            format!("glTF {source}"),
            Some(me),
            work,
            move |e, got| arrived(e, me, generation, &source, flatten(got)),
        );
        if spawned.is_err() {
            refuse_pool_full(e, me, generation);
        }
    }

    fn current(e: &Engine, me: ObjectId, generation: u64) -> bool {
        e.is_alive(me)
            && e.with(me, |l: &RuntimeLoader| l.generation == generation)
                .unwrap_or(false)
    }

    fn arrived(
        e: &Engine,
        me: ObjectId,
        generation: u64,
        source: &str,
        built: Result<Asset, String>,
    ) {
        if !current(e, me, generation) {
            return;
        }
        let asset = match built {
            Ok(asset) => asset,
            Err(why) => {
                let error = format!("{source}: {why}");
                e.log(LogLevel::Warning, CATEGORY, error.clone());
                finish(e, me, status::ERROR, error);
                return;
            }
        };
        for warning in &asset.warnings {
            e.log(LogLevel::Warning, CATEGORY, format!("{source}: {warning}"));
        }
        let bounds = asset
            .bounds
            .map(|(min, max)| (to_value(min), to_value(max)));
        let asset = Arc::new(asset);
        let _ = e.with_mut(me, |l: &mut RuntimeLoader| l.asset = Some(asset));
        set_bounds(e, me, bounds);
        finish(e, me, status::SUCCESS, String::new());
    }

    /// `rel` resolved against the URL of the file that names it. glTF
    /// names its files by relative URI references; one with a scheme is
    /// taken as it is.
    pub(super) fn join(base: &str, rel: &str) -> String {
        let scheme = rel
            .find(':')
            .is_some_and(|i| i > 0 && !rel[..i].contains(['/', '?', '#']));
        if scheme {
            return rel.to_owned();
        }
        let base = base.split(['?', '#']).next().unwrap_or(base);
        if let Some(abs) = rel.strip_prefix('/') {
            // From the root of the base's authority.
            let after_scheme = base.find("://").map_or(0, |i| i + 3);
            let root = base[after_scheme..]
                .find('/')
                .map_or(base.len(), |i| after_scheme + i);
            return format!("{}/{abs}", &base[..root]);
        }
        let dir = base.rfind('/').map_or("", |i| &base[..=i]);
        let mut out: Vec<&str> = dir.split('/').collect();
        // `dir` ends in '/', so its last part is empty: the place for `rel`.
        out.pop();
        for part in rel.split('/') {
            match part {
                "." => {}
                ".." => {
                    // Never above the authority (`scheme://host`).
                    if out.len() > 3 || (out.len() > 1 && !dir.contains("://")) {
                        out.pop();
                    }
                }
                p => out.push(p),
            }
        }
        out.join("/")
    }
}

#[cfg(all(test, feature = "gltf"))]
mod tests {
    use super::gltf_load::join;

    #[test]
    fn relative_uris_resolve_against_the_file() {
        assert_eq!(join("file:///a/b/m.gltf", "m.bin"), "file:///a/b/m.bin");
        assert_eq!(
            join("file:///a/b/m.gltf", "tex/c.png"),
            "file:///a/b/tex/c.png"
        );
        assert_eq!(join("file:///a/b/m.gltf", "../c.png"), "file:///a/c.png");
        assert_eq!(
            join("file:///a/b/m.gltf", "./c%20d.png"),
            "file:///a/b/c%20d.png"
        );
        assert_eq!(join("https://h/x/m.gltf?v=1", "m.bin"), "https://h/x/m.bin");
        assert_eq!(join("https://h/x/m.gltf", "/r/m.bin"), "https://h/r/m.bin");
        assert_eq!(join("https://h/m.gltf", "../../m.bin"), "https://h/m.bin");
        assert_eq!(join("qrc:/m/a.gltf", "a.bin"), "qrc:/m/a.bin");
        assert_eq!(
            join("file:///a/m.gltf", "https://h/m.bin"),
            "https://h/m.bin"
        );
    }
}
