//! `RuntimeLoader`: a node whose children come from a glTF or Wavefront OBJ
//! file read while the application runs, told apart by its extension.
//!
//! The file is read and parsed on the compute pool. A `.gltf` that names
//! other files (buffers, images) is parsed first; the main thread then
//! finds those files as it finds any source (a resource, a local path or
//! a fetch) and the pool reads and builds the model from them. An `.obj`
//! goes the same way twice: its `.mtl` files, then their pictures. A late
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
/// (`.gltf` with its files, or `.glb`) or a Wavefront OBJ file (`.obj` with
/// its `.mtl` files and their pictures), as children of this node.
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
    /// Copies the whole file: each copy is the file's content placed by
    /// an entry in this node's space.
    #[qml(notify, object = "Instancing")]
    pub instancing: Option<ObjectId>,
    #[qml(skip)]
    generation: u64,
    /// A `Model` for each of the file's nodes that has a mesh, named
    /// after it, with the node's index: what a pick's `objectHit` is.
    #[qml(skip)]
    models: Vec<(usize, ObjectId)>,
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

/// The models made for the loader `obj`'s meshes, by the index of the
/// file's node each is.
#[cfg(feature = "gltf")]
pub(crate) fn models(e: &Engine, obj: ObjectId) -> Vec<(usize, ObjectId)> {
    e.with(obj, |l: &RuntimeLoader| l.models.clone())
        .unwrap_or_default()
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
            (l.generation, std::mem::take(&mut l.models))
        })
    }) else {
        return;
    };
    let (generation, models) = generation;
    for (_, m) in models {
        e.destroy(m);
    }
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
        format!("{source}: this build reads no model files (the `gltf` feature is off)"),
    );
}

#[cfg(feature = "gltf")]
use gltf_load::start;

#[cfg(feature = "gltf")]
mod gltf_load {
    use std::sync::Arc;

    use ljos::item::ljos_3d::fetch::{Fetch, Fetched, Files, fetch_all};
    use ljos::item::ljos_3d::gltf::{self, Asset, Document};
    use ljos::item::ljos_3d::obj::{self, Reader};
    use ljos::prelude::*;

    use crate::fetch::EngineFetch;

    use super::{RuntimeLoader, finish, set_bounds, status};
    use crate::space::to_value;

    const CATEGORY: &str = "qt.quick3d.runtimeloader";

    /// What the first pass on the pool carries back.
    enum Parsed {
        Built(Result<Asset, String>),
        /// The document, and the files it names that must be fetched first.
        Needs(Box<Document>, Vec<String>),
    }

    pub(super) fn start(e: &Engine, me: ObjectId, generation: u64, source: String) {
        let url = e.resolve_url(me, &source);
        let Some(reader) = obj::reader_for(&url) else {
            let why = "only .obj, .gltf and .glb files are read".to_owned();
            return arrived(e, me, generation, &source, Err(why));
        };
        let base = url.clone();
        EngineFetch::pooled(me).fetch(
            e,
            &url,
            Box::new(move |e, got| {
                if current(e, me, generation) {
                    match reader {
                        Reader::Gltf => parse(e, me, generation, source, base, got),
                        Reader::Obj => parse_obj(e, me, generation, source, base, got),
                    }
                }
            }),
        );
    }

    /// A step of reading an OBJ file on the pool, and what follows it back
    /// here if the source is still the same.
    fn obj_step<T: Send + 'static>(
        e: &Engine,
        me: ObjectId,
        generation: u64,
        source: String,
        work: impl FnOnce() -> Result<T, String> + Send + 'static,
        next: impl FnOnce(&Engine, String, T) + 'static,
    ) {
        let spawned =
            ljos::task::spawn_for(e, format!("OBJ {source}"), Some(me), work, move |e, got| {
                match flatten(got) {
                    Ok(done) if current(e, me, generation) => next(e, source, done),
                    Ok(_) => {}
                    Err(why) => arrived(e, me, generation, &source, Err(why)),
                }
            });
        if spawned.is_err() {
            refuse_pool_full(e, me, generation);
        }
    }

    /// The `.obj`, its `.mtl` files, then their pictures: each answer names
    /// what the next step fetches.
    fn parse_obj(
        e: &Engine,
        me: ObjectId,
        generation: u64,
        source: String,
        base: String,
        got: Fetched,
    ) {
        let work = move || obj::parse(&got?).map_err(|err| err.to_string());
        obj_step(e, me, generation, source, work, move |e, source, doc| {
            let fetch = EngineFetch::pooled(me);
            fetch_all(
                &fetch,
                e,
                &base.clone(),
                doc.libraries(),
                move |e, files| {
                    if current(e, me, generation) {
                        obj_materials(e, me, generation, source, base, doc, files);
                    }
                },
            );
        });
    }

    fn obj_materials(
        e: &Engine,
        me: ObjectId,
        generation: u64,
        source: String,
        base: String,
        mut doc: obj::Document,
        mut files: Files,
    ) {
        let work = move || {
            doc.read_libraries(&base, &mut |uri| files.take(uri));
            Ok(doc)
        };
        obj_step(e, me, generation, source, work, move |e, source, doc| {
            let fetch = EngineFetch::pooled(me);
            // Already whole URLs: each resolved against its `.mtl`.
            fetch_all(&fetch, e, "", doc.image_urls(), move |e, mut files| {
                if !current(e, me, generation) {
                    return;
                }
                let work = move || {
                    doc.build(&mut |uri| files.take(uri))
                        .map_err(|err| err.to_string())
                };
                obj_step(e, me, generation, source, work, move |e, source, asset| {
                    arrived(e, me, generation, &source, Ok(asset));
                });
            });
        });
    }

    fn parse(
        e: &Engine,
        me: ObjectId,
        generation: u64,
        source: String,
        base: String,
        got: Fetched,
    ) {
        let work = move || -> Result<Parsed, String> {
            let doc = gltf::parse(&got?).map_err(|err| err.to_string())?;
            let uris = doc.external_uris();
            if uris.is_empty() {
                Ok(Parsed::Built(
                    doc.build(&mut |uri| Files::default().take(uri))
                        .map_err(|e| e.to_string()),
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
                        let fetch = EngineFetch::pooled(me);
                        fetch_all(&fetch, e, &base, uris, move |e, files| {
                            if current(e, me, generation) {
                                build(e, me, generation, source, doc, files);
                            }
                        });
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

    /// Builds the model on the pool from the files the document names.
    fn build(
        e: &Engine,
        me: ObjectId,
        generation: u64,
        source: String,
        doc: Box<Document>,
        mut files: Files,
    ) {
        let work = move || -> Result<Asset, String> {
            doc.build(&mut |uri| files.take(uri))
                .map_err(|err| err.to_string())
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
        let models = made_models(e, me, &asset);
        let asset = Arc::new(asset);
        let _ = e.with_mut(me, |l: &mut RuntimeLoader| {
            l.asset = Some(asset);
            l.models = models;
        });
        set_bounds(e, me, bounds);
        finish(e, me, status::SUCCESS, String::new());
    }

    /// A pickable `Model` for each node of `asset` with a mesh, owned by
    /// the loader. They are not in its `data`, so a walk does not draw
    /// them: the file's meshes are drawn from the asset.
    fn made_models(e: &Engine, me: ObjectId, asset: &Asset) -> Vec<(usize, ObjectId)> {
        asset
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.mesh.is_some())
            .filter_map(|(i, n)| {
                let mut model = crate::Model {
                    pickable: true,
                    ..Default::default()
                };
                model.node.object3d.object.object_name =
                    Atom::new(n.name.as_deref().unwrap_or_default());
                e.create_native(model, Some(me)).ok().map(|m| (i, m))
            })
            .collect()
    }
}
